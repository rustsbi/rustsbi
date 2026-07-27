//! Two-hart handoff protocol for starting a stopped hart.

use super::admission::AdmissionError;
use super::instructions::protocol_fence;
use super::protocol::{HartAdmission, map_hart_error};
use crate::boot::{NextMode, NextStage};
use crate::hart::{HartError, HartStatus};

/// Source/target agreement for transferring one validated next-stage entry.
///
/// The requesting hart first waits for target-local machine preparation. A
/// prepared target may consume the entry only after the requester publishes
/// `proceed`; a failed preparation instead returns the slot to its empty state.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub(crate) struct StartHandshake {
    pub(super) result: StartResult,
    pub(super) proceed: bool,
}

/// Preparation result published by the hart being started.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub(super) enum StartResult {
    /// The target has not completed machine-local preparation.
    #[default]
    Waiting,
    /// Preparation succeeded and the target is waiting for permission to enter.
    Prepared,
    /// Preparation failed and the requester must cancel the start transition.
    Failed,
}

/// Entry authority and diagnostic retained while one start is pending.
pub(super) struct PendingStart {
    // `Some` owns the target entry until a successful target consume. `None`
    // means that the target has claimed the entry for immediate handoff.
    pub(super) next_stage: Option<NextStage>,
    pub(super) handshake: StartHandshake,
    pub(super) failure: Option<HartError>,
}

impl StartHandshake {
    /// Publishes successful target-local preparation exactly once.
    pub(crate) fn publish_prepared(&mut self) -> Result<(), AdmissionError> {
        if self.result != StartResult::Waiting || self.proceed {
            return Err(AdmissionError::InvalidTransition);
        }
        self.result = StartResult::Prepared;
        Ok(())
    }

    /// Publishes failed target-local preparation exactly once.
    pub(crate) fn publish_failed(&mut self) -> Result<(), AdmissionError> {
        if self.result != StartResult::Waiting || self.proceed {
            return Err(AdmissionError::InvalidTransition);
        }
        self.result = StartResult::Failed;
        Ok(())
    }

    /// Allows a prepared target to consume its next-stage entry.
    pub(crate) fn source_proceed(&mut self) -> Result<(), AdmissionError> {
        if self.result != StartResult::Prepared || self.proceed {
            return Err(AdmissionError::InvalidTransition);
        }
        self.proceed = true;
        Ok(())
    }

    /// Completes a successful handoff and resets the reusable handshake slot.
    pub(crate) fn target_consume(&mut self) -> Result<(), AdmissionError> {
        if self.result != StartResult::Prepared || !self.proceed {
            return Err(AdmissionError::InvalidTransition);
        }
        *self = Self::default();
        Ok(())
    }

    /// Acknowledges target preparation failure and resets the handshake slot.
    pub(crate) fn source_observed_failure(&mut self) -> Result<(), AdmissionError> {
        if self.result != StartResult::Failed || self.proceed {
            return Err(AdmissionError::InvalidTransition);
        }
        *self = Self::default();
        Ok(())
    }
}

impl HartAdmission {
    /// Returns the privilege mode requested by a pending start operation.
    pub(crate) fn pending_start_mode(&self, hart_id: usize) -> Option<NextMode> {
        let state = self.state.lock();
        let target = state.resolve_physical(hart_id).ok()?;
        if state.state(target).ok()? != HartStatus::StartPending {
            return None;
        }
        state.starts[target]
            .as_ref()?
            .next_stage
            .as_ref()
            .map(NextStage::mode)
    }

    /// Starts a stopped hart after its target-local preparation succeeds.
    pub(crate) fn start(&self, hart_id: usize, next_stage: NextStage) -> Result<(), HartError> {
        let (target, physical) = {
            let mut state = self.state.lock();
            let target = state.resolve_physical(hart_id).map_err(map_hart_error)?;
            match state.state(target).map_err(map_hart_error)? {
                HartStatus::Stopped => {}
                HartStatus::Started | HartStatus::StartPending => {
                    return Err(HartError::AlreadyAvailable);
                }
                HartStatus::StopPending
                | HartStatus::Suspended
                | HartStatus::SuspendPending
                | HartStatus::ResumePending => return Err(HartError::Failed),
            }
            if state.starts[target].is_some() {
                return Err(HartError::Failed);
            }

            state.begin_start(target).map_err(map_hart_error)?;
            state.starts[target] = Some(PendingStart {
                next_stage: Some(next_stage),
                handshake: StartHandshake::default(),
                failure: None,
            });

            let physical = state.committed_physical_id(target);
            (target, physical)
        };
        // The start request and its entry authority are fully committed before
        // ringing the target. Device latency can never extend the admission
        // critical section.
        protocol_fence();
        self.device.notify(physical);
        protocol_fence();

        loop {
            let mut state = self.state.lock();
            let result = state.starts[target]
                .as_ref()
                .map(|slot| slot.handshake.result)
                .ok_or(HartError::Failed)?;
            match result {
                StartResult::Waiting => {}
                StartResult::Prepared => {
                    state.starts[target]
                        .as_mut()
                        .ok_or(HartError::Failed)?
                        .handshake
                        .source_proceed()
                        .map_err(map_hart_error)?;
                    return Ok(());
                }
                StartResult::Failed => {
                    let error = state.starts[target]
                        .as_mut()
                        .and_then(|slot| slot.failure.take())
                        .unwrap_or(HartError::Failed);
                    state.starts[target]
                        .as_mut()
                        .ok_or(HartError::Failed)?
                        .handshake
                        .source_observed_failure()
                        .map_err(map_hart_error)?;
                    state.cancel_start(target).map_err(map_hart_error)?;
                    state.starts[target] = None;
                    return Err(error);
                }
            }
            drop(state);
            core::hint::spin_loop();
        }
    }

    /// Publishes whether the target hart completed its machine preparation.
    pub(crate) fn publish_start_result(
        &self,
        hart_id: usize,
        result: Result<(), HartError>,
    ) -> Result<(), HartError> {
        let mut state = self.state.lock();
        let target = state.resolve_physical(hart_id).map_err(map_hart_error)?;
        if state.state(target).map_err(map_hart_error)? != HartStatus::StartPending {
            return Err(HartError::Failed);
        }
        let slot = state.starts[target].as_mut().ok_or(HartError::Failed)?;
        match result {
            Ok(()) => slot.handshake.publish_prepared().map_err(map_hart_error),
            Err(error) => {
                slot.failure = Some(error);
                if let Err(protocol_error) = slot.handshake.publish_failed() {
                    slot.failure = None;
                    return Err(map_hart_error(protocol_error));
                }
                Ok(())
            }
        }
    }

    /// Transfers a prepared next-stage entry to the target hart exactly once.
    pub(crate) fn take_start(&self, hart_id: usize) -> Result<NextStage, HartError> {
        loop {
            let mut state = self.state.lock();
            let target = state.resolve_physical(hart_id).map_err(map_hart_error)?;
            let proceed = state.starts[target].as_ref().is_some_and(|slot| {
                slot.handshake.result == StartResult::Prepared
                    && slot.handshake.proceed
                    && slot.next_stage.is_some()
            });
            if proceed {
                // Every rejection point was checked under the same ticket. The
                // target now consumes entry authority without a fallible gap.
                let slot = match state.starts[target].as_mut() {
                    Some(slot) => slot,
                    None => crate::trap::abort(),
                };
                if slot.handshake.target_consume().is_err() {
                    crate::trap::abort();
                }
                let next_stage = match slot.next_stage.take() {
                    Some(next_stage) => next_stage,
                    None => crate::trap::abort(),
                };
                if state.complete_start(target).is_err() {
                    crate::trap::abort();
                }
                state.starts[target] = None;
                return Ok(next_stage);
            }
            drop(state);
            core::hint::spin_loop();
        }
    }
}
