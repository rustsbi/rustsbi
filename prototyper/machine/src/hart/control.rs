//! Safe hart lifecycle capability.

use alloc::sync::Arc;

use super::admission::{AdmissionError, ClaimedWork};
use super::instructions::{clear_supervisor_ipi, current_hart_id, wait_for_wake_event};
use super::protocol::{HartAdmission, map_hart_error};
use crate::boot::NextStage;

/// Ratified hart lifecycle state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum HartStatus {
    /// The hart is executing its next stage.
    Started,
    /// The hart is parked in machine mode.
    Stopped,
    /// A start operation has been accepted but is not yet complete.
    StartPending,
    /// The hart is draining accepted work before stopping.
    StopPending,
    /// The hart is suspended.
    Suspended,
    /// A suspend operation has been accepted but is not yet complete.
    SuspendPending,
    /// A suspended hart is resuming.
    ResumePending,
}

/// Failure from a hart lifecycle operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HartError {
    /// The physical hart ID is not admitted.
    InvalidHart,
    /// The requested next-stage address is invalid.
    InvalidAddress,
    /// The target is already started or being started.
    AlreadyAvailable,
    /// The platform cannot perform the requested lifecycle operation.
    NotSupported,
    /// A machine lifecycle invariant or mechanism failed.
    Failed,
}

/// Authority to inspect and change admitted hart lifecycle state.
pub struct HartControl {
    admission: Arc<HartAdmission>,
}

impl HartControl {
    pub(crate) fn new(admission: Arc<HartAdmission>) -> Self {
        Self { admission }
    }

    /// Starts one stopped hart at the validated next stage.
    pub fn start(&self, hart_id: usize, next_stage: NextStage) -> Result<(), HartError> {
        self.admission.start(hart_id, next_stage)
    }

    /// Returns the current ratified lifecycle state of one admitted hart.
    pub fn status(&self, hart_id: usize) -> Result<HartStatus, HartError> {
        self.admission.status(hart_id)
    }

    /// Stops the calling hart. A successful stop does not return.
    pub fn stop(&self) -> HartError {
        match self.admission.stop_current() {
            Ok(()) => crate::trap::park_current_hart(),
            Err(error) => error,
        }
    }

    /// Enters a retentive suspend and returns after resume.
    pub fn suspend_retentive(&self) -> Result<(), HartError> {
        self.admission.suspend_current()
    }

    /// Enters a non-retentive suspend. A successful resume enters `next_stage`.
    /// Suspends the current hart and enters `next_stage` after an IPI wakeup.
    ///
    /// The current implementation uses a retaining machine `WFI`; therefore
    /// counter, interrupt, protection, and trap state remain installed. A
    /// successful operation abandons the interrupted supervisor frame and
    /// cannot return to the original SBI call.
    pub fn suspend_non_retentive(&self, next_stage: NextStage) -> HartError {
        match self.admission.suspend_current() {
            Ok(()) => crate::trap::enter_resumed_stage(next_stage),
            Err(error) => error,
        }
    }

    /// Suspends the complete supervisor-visible system and enters `next_stage`
    /// after wakeup.
    ///
    /// The operation is accepted only when every admitted peer hart is stopped.
    /// That predicate and the calling hart's suspend transition share one lower
    /// protocol commit, so concurrent lifecycle calls cannot invalidate it.
    pub fn suspend_system(&self, next_stage: NextStage) -> HartError {
        match self.admission.suspend_system() {
            Ok(()) => crate::trap::enter_resumed_stage(next_stage),
            Err(error) => error,
        }
    }
}

impl HartAdmission {
    /// Returns the locked lifecycle state of one admitted physical hart.
    pub(crate) fn status(&self, hart_id: usize) -> Result<HartStatus, HartError> {
        let state = self.state.lock();
        let index = state.resolve_physical(hart_id).map_err(map_hart_error)?;
        state.state(index).map_err(map_hart_error)
    }

    /// Gates new work, drains accepted work, and stops the calling hart.
    pub(crate) fn stop_current(&self) -> Result<(), HartError> {
        let physical = current_hart_id();
        let target = {
            let mut state = self.state.lock();
            let target = state.resolve_physical(physical).map_err(map_hart_error)?;
            state.begin_stop(target).map_err(map_hart_error)?;
            target
        };

        // Once the stop gate commits, failure must not return to the old
        // supervisor context; all previously accepted work belongs to this drain.
        if self.drain(physical, true).is_err() {
            crate::trap::abort();
        }
        clear_supervisor_ipi();
        let mut state = self.state.lock();
        if state.finish_stop(target, &ClaimedWork::default()).is_err() {
            crate::trap::abort();
        }
        Ok(())
    }

    /// Suspends the calling hart using its validated notification wake path.
    pub(crate) fn suspend_current(&self) -> Result<(), HartError> {
        let physical = current_hart_id();
        let target = {
            let mut state = self.state.lock();
            let target = state.resolve_physical(physical).map_err(map_hart_error)?;
            if !state.wakeable_by_ipi(target).map_err(map_hart_error)? {
                return Err(HartError::NotSupported);
            }
            state.begin_suspend(target).map_err(map_hart_error)?;
            target
        };
        self.complete_current_suspend(physical, target)
    }

    /// Suspends the calling hart only when every admitted peer is stopped.
    pub(crate) fn suspend_system(&self) -> Result<(), HartError> {
        let physical = current_hart_id();
        let target = {
            let mut state = self.state.lock();
            let target = state.resolve_physical(physical).map_err(map_hart_error)?;
            if !state.wakeable_by_ipi(target).map_err(map_hart_error)? {
                return Err(HartError::NotSupported);
            }
            match state.begin_system_suspend(target) {
                Ok(()) => {}
                Err(AdmissionError::Unavailable) => return Err(HartError::AlreadyAvailable),
                Err(error) => return Err(map_hart_error(error)),
            }
            target
        };
        self.complete_current_suspend(physical, target)
    }

    fn complete_current_suspend(&self, physical: usize, target: usize) -> Result<(), HartError> {
        let drained = match self.drain(physical, true) {
            Ok(drained) => drained,
            Err(_) => crate::trap::abort(),
        };
        {
            let mut state = self.state.lock();
            if state.finish_suspend(target).is_err() {
                crate::trap::abort();
            }
        }

        // An IPI accepted before the suspend gate is already a wake reason.
        if drained.supervisor_interrupt {
            let mut state = self.state.lock();
            if state.begin_resume(target).is_err() || state.finish_resume(target).is_err() {
                crate::trap::abort();
            }
            return Ok(());
        }

        wait_for_wake_event(self.notification());
        {
            let mut state = self.state.lock();
            if state.begin_resume(target).is_err() {
                crate::trap::abort();
            }
        }
        if self.drain(physical, true).is_err() {
            crate::trap::abort();
        }
        let mut state = self.state.lock();
        if state.finish_resume(target).is_err() {
            crate::trap::abort();
        }
        Ok(())
    }
}
