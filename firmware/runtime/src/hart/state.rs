//! Hart start, stop, and suspend state transitions.

use crate::{Error, Result};
use core::cell::UnsafeCell;
use core::hint::spin_loop;
use core::sync::atomic::{AtomicU8, Ordering};

use super::{Hart, HartId};
use crate::boot::NextStage;
use crate::irq::LocalIrqDisabled;

// State values are private Runtime facts, not SBI state IDs.
const STATE_STARTED: u8 = 0;
const STATE_STOPPED: u8 = 1;
const STATE_START_PENDING: u8 = 2;
const STATE_STOPPING: u8 = 3;
const STATE_SUSPENDED: u8 = 4;
const STATE_RESUME_PENDING: u8 = 6;
const STATE_STARTING: u8 = u8::MAX;

#[repr(align(128))]
pub(super) struct HartStateCell {
    state: AtomicU8,
    stage: UnsafeCell<Option<NextStage>>,
    transfer: UnsafeCell<Option<ControlTransfer>>,
}

impl HartStateCell {
    pub(super) const fn new() -> Self {
        Self {
            state: AtomicU8::new(STATE_STOPPED),
            stage: UnsafeCell::new(None),
            transfer: UnsafeCell::new(None),
        }
    }
}

// SAFETY: state publication is atomic; staged values are only written while
// a private reservation is held and are consumed by the owning hart.
unsafe impl Sync for HartStateCell {}

/// Protocol-independent hart lifecycle state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HartState {
    /// The hart is executing normally.
    Started,
    /// The hart is parked outside lower privilege modes.
    Stopped,
    /// A start handoff is being published or is waiting for the hart.
    StartPending,
    /// A stop is committed and awaits retirement of the policy call.
    StopPending,
    /// The hart is in a retentive platform suspend state.
    Suspended,
    /// A non-retentive resume is being prepared.
    ResumePending,
}

/// Work observed by the machine software/external interrupt transport.
pub(crate) enum HartEvent {
    /// Enter the staged lower-privilege execution stage.
    Start(NextStage),
    /// Park the current hart until another start arrives.
    Park,
    /// Deliver ordinary platform software-interrupt work.
    None,
}

/// Result of reserving a stopped hart for a start request.
pub enum StartOutcome {
    /// The request owns the start handoff.
    Accepted,
    /// The hart was not stopped and could not be reserved.
    AlreadyRunning,
}

/// A machine control transfer staged by a successful non-retentive resume.
pub(crate) enum ControlTransfer {
    /// Retire the supervisor context after the SBI policy call has returned.
    Stop,
    /// Enter the supplied lower-privilege stage from the ecall return path.
    NonRetentiveResume(NextStage),
}

impl Hart {
    /// Stages the initial boot handoff for the current hart.
    pub fn stage_boot_handoff(&self, stage: NextStage) -> Result<()> {
        self.require_current()?;
        let cell = &self.state;
        if cell
            .state
            .compare_exchange(
                STATE_STOPPED,
                STATE_STARTING,
                Ordering::Acquire,
                Ordering::Relaxed,
            )
            .is_err()
        {
            return Err(Error::Busy);
        }
        // SAFETY: STARTING reserves the slot for this hart's boot path.
        unsafe { *cell.stage.get() = Some(stage) };
        cell.state.store(STATE_START_PENDING, Ordering::Release);
        Ok(())
    }

    /// Reserves and wakes a stopped hart, publishing its stage only after wake.
    pub fn start(&self, stage: NextStage) -> Result<StartOutcome> {
        let cell = &self.state;
        if cell
            .state
            .compare_exchange(
                STATE_STOPPED,
                STATE_STARTING,
                Ordering::Acquire,
                Ordering::Relaxed,
            )
            .is_err()
        {
            return Ok(StartOutcome::AlreadyRunning);
        }
        if let Err(error) = super::wakeup::wake(self.id()) {
            cell.state.store(STATE_STOPPED, Ordering::Release);
            return Err(error);
        }
        // SAFETY: STARTING reserves the slot until the Release publication.
        unsafe { *cell.stage.get() = Some(stage) };
        cell.state.store(STATE_START_PENDING, Ordering::Release);
        Ok(StartOutcome::Accepted)
    }

    /// Consumes a local start or park event.
    pub(crate) fn take_local_event(&self) -> HartEvent {
        assert_eq!(self.id(), super::current_hart());
        let cell = &self.state;
        loop {
            match cell.state.load(Ordering::Acquire) {
                STATE_START_PENDING => {
                    if cell
                        .state
                        .compare_exchange(
                            STATE_START_PENDING,
                            STATE_STARTED,
                            Ordering::AcqRel,
                            Ordering::Relaxed,
                        )
                        .is_ok()
                    {
                        // SAFETY: the AcqRel transition consumes the published
                        // stage, and only this hart consumes its local stage.
                        let stage = unsafe { (*cell.stage.get()).take() }
                            .expect("BUG: start state without staged handoff");
                        return HartEvent::Start(stage);
                    }
                }
                STATE_STARTING => spin_loop(),
                STATE_STOPPED => return HartEvent::Park,
                _ => return HartEvent::None,
            }
        }
    }

    /// Reads a hart's protocol-independent lifecycle state.
    #[inline]
    pub fn status(&self) -> HartState {
        match self.state.state.load(Ordering::Acquire) {
            STATE_STARTED => HartState::Started,
            STATE_STOPPED => HartState::Stopped,
            STATE_START_PENDING | STATE_STARTING => HartState::StartPending,
            STATE_STOPPING => HartState::StopPending,
            STATE_SUSPENDED => HartState::Suspended,
            STATE_RESUME_PENDING => HartState::ResumePending,
            _ => unreachable!("BUG: invalid Runtime hart state"),
        }
    }

    /// Returns whether the hart can receive ordinary firmware IPIs.
    #[inline]
    pub fn can_receive_ipi(&self) -> bool {
        matches!(self.status(), HartState::Started | HartState::Suspended)
    }

    /// Requests a stop at the enclosing SBI call's return to Runtime.
    /// Policy must return after success so its Rust call chain can unwind normally.
    /// Runtime then parks until a new start without returning to the old supervisor.
    pub fn stop(&self) -> Result<()> {
        self.require_current()?;
        let ipi = crate::boot::ipi().ok_or(Error::IoError)?;
        ipi.delivery().ok_or(Error::IoError)?;
        crate::irq::with_local_disabled(|irq| {
            self.state
                .state
                .compare_exchange(
                    STATE_STARTED,
                    STATE_STOPPING,
                    Ordering::AcqRel,
                    Ordering::Relaxed,
                )
                .map_err(|_| Error::InvalidState)?;
            if let Err(error) = ipi.clear_current(irq) {
                self.state.state.store(STATE_STARTED, Ordering::Release);
                return Err(error);
            }
            ipi.prepare_wakeup(irq);
            // SAFETY: This hart owns its transfer slot and excludes IRQ reentry.
            unsafe { *self.state.transfer.get() = Some(ControlTransfer::Stop) };
            Ok(())
        })
    }

    /// Enters the platform suspend wait for the current hart.
    pub fn suspend(&self) -> Result<()> {
        self.require_current()?;
        let ipi = crate::boot::ipi().ok_or(Error::IoError)?;
        ipi.delivery().ok_or(Error::IoError)?;
        crate::irq::with_local_disabled(|irq| {
            self.state
                .state
                .compare_exchange(
                    STATE_STARTED,
                    STATE_SUSPENDED,
                    Ordering::AcqRel,
                    Ordering::Relaxed,
                )
                .map_err(|_| Error::InvalidState)?;
            ipi.clear_current(irq)?;
            let notified = ipi.deliver_current(irq);
            ipi.prepare_wakeup(irq);
            // An IPI consumed above has already become SSIP. It must still
            // wake this suspend call when the supervisor has disabled SSIE.
            if !notified {
                riscv::asm::wfi();
            }
            Ok(())
        })
        .inspect_err(|_| {
            let _ = self.state.state.compare_exchange(
                STATE_SUSPENDED,
                STATE_STARTED,
                Ordering::AcqRel,
                Ordering::Relaxed,
            );
        })
    }

    /// Completes a retentive resume on the current hart.
    pub fn resume_retentive(&self) -> Result<()> {
        self.require_current()?;
        let cell = &self.state;
        if cell
            .state
            .compare_exchange(
                STATE_SUSPENDED,
                STATE_STARTED,
                Ordering::AcqRel,
                Ordering::Relaxed,
            )
            .is_err()
        {
            return Err(Error::InvalidState);
        }
        Ok(())
    }

    /// Commits a non-retentive handoff after the platform wait has returned.
    /// A failed wake returns to the original supervisor with Started restored.
    pub fn resume_nonretentive(&self, stage: NextStage) -> Result<()> {
        self.require_current()?;
        crate::irq::with_local_disabled(|_| {
            self.state
                .state
                .compare_exchange(
                    STATE_SUSPENDED,
                    STATE_RESUME_PENDING,
                    Ordering::AcqRel,
                    Ordering::Relaxed,
                )
                .map_err(|_| Error::InvalidState)?;
            let result = crate::boot::ipi()
                .ok_or(Error::IoError)
                .and_then(|ipi| ipi.send(crate::ipi::IpiRequest::for_hart(self.id())));
            if result.is_ok() {
                // SAFETY: This hart owns its transfer slot with IRQs excluded.
                unsafe {
                    *self.state.transfer.get() = Some(ControlTransfer::NonRetentiveResume(stage))
                };
            }
            self.state.state.store(STATE_STARTED, Ordering::Release);
            result
        })
    }

    fn require_current(&self) -> Result<()> {
        if self.id() != HartId::current()? {
            return Err(Error::InvalidArgs);
        }
        if !crate::boot::traps_ready() || usize::from(csr::machine::Scratch::read()) != 0 {
            return Err(Error::NotReady);
        }
        Ok(())
    }

    /// Waits on this hart's transport without discarding live Rust stack frames.
    pub(crate) fn wait_for_start(&self, irq: &LocalIrqDisabled) -> NextStage {
        let ipi = crate::boot::ipi().expect("IPI service not published");
        if self.state.state.load(Ordering::Acquire) == STATE_STOPPING {
            self.state.state.store(STATE_STOPPED, Ordering::Release);
        }
        loop {
            if ipi.delivery().is_some() {
                ipi.clear_current(irq)
                    .expect("could not clear hart wake interrupt");
                riscv::asm::fence();
            }
            let event = self.take_local_event();
            ipi.deliver_current(irq);
            match event {
                HartEvent::Start(stage) => return stage,
                HartEvent::Park => {
                    ipi.prepare_wakeup(irq);
                    riscv::asm::wfi();
                }
                HartEvent::None => unreachable!("waiting hart is neither stopped nor starting"),
            }
        }
    }

    /// Takes the committed resume marker on this hart's ecall return path.
    pub(crate) fn take_control_transfer(&self) -> Option<ControlTransfer> {
        assert_eq!(self.id(), super::current_hart());
        // SAFETY: Only this hart consumes its own marker in M-mode.
        unsafe { (*self.state.transfer.get()).take() }
    }
}
