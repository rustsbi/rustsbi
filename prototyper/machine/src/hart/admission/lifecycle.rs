//! Ratified HSM lifecycle transitions and stop/suspend gates.

use super::*;

impl<const HARTS: usize> HartAdmissionState<HARTS> {
    pub(crate) fn begin_start(&mut self, target: usize) -> Result<(), AdmissionError> {
        self.transition(target, HartState::Stopped, HartState::StartPending)
    }

    pub(crate) fn complete_start(&mut self, target: usize) -> Result<(), AdmissionError> {
        self.transition(target, HartState::StartPending, HartState::Started)
    }

    pub(crate) fn cancel_start(&mut self, target: usize) -> Result<(), AdmissionError> {
        self.transition(target, HartState::StartPending, HartState::Stopped)
    }

    pub(crate) fn begin_stop(&mut self, target: usize) -> Result<(), AdmissionError> {
        self.transition(target, HartState::Started, HartState::StopPending)
    }

    pub(crate) fn finish_stop(
        &mut self,
        target: usize,
        claimed: &ClaimedWork,
    ) -> Result<(), AdmissionError> {
        if !claimed.is_empty() {
            return Err(AdmissionError::BatchBusy);
        }
        self.require_no_work(target)?;
        self.transition(target, HartState::StopPending, HartState::Stopped)
    }

    pub(crate) fn begin_suspend(&mut self, target: usize) -> Result<(), AdmissionError> {
        self.transition(target, HartState::Started, HartState::SuspendPending)
    }

    /// Commits system suspend only while every peer is stopped under this same
    /// admission lock.
    pub(crate) fn begin_system_suspend(&mut self, target: usize) -> Result<(), AdmissionError> {
        if target >= self.hart_count {
            return Err(AdmissionError::InvalidHart);
        }
        if self.lifecycle[..self.hart_count]
            .iter()
            .enumerate()
            .any(|(index, state)| index != target && state.status != HartState::Stopped)
        {
            return Err(AdmissionError::Unavailable);
        }
        self.begin_suspend(target)
    }

    pub(crate) fn finish_suspend(&mut self, target: usize) -> Result<(), AdmissionError> {
        self.require_no_work(target)?;
        self.transition(target, HartState::SuspendPending, HartState::Suspended)
    }

    pub(crate) fn wakeable_by_ipi(&self, target: usize) -> Result<bool, AdmissionError> {
        self.lifecycle[..self.hart_count]
            .get(target)
            .map(|state| state.wake_by_ipi)
            .ok_or(AdmissionError::InvalidHart)
    }

    pub(crate) fn begin_resume(&mut self, target: usize) -> Result<(), AdmissionError> {
        self.transition(target, HartState::Suspended, HartState::ResumePending)
    }

    pub(crate) fn finish_resume(&mut self, target: usize) -> Result<(), AdmissionError> {
        self.transition(target, HartState::ResumePending, HartState::Started)
    }

    fn require_no_work(&self, target: usize) -> Result<(), AdmissionError> {
        if target >= self.hart_count {
            return Err(AdmissionError::InvalidHart);
        }
        if self.ipi[target].pending
            || !self.fence_targets[target].pending_sources.is_empty()
            || self.fence_sources[target].active.is_some()
        {
            return Err(AdmissionError::MissingRelation);
        }
        Ok(())
    }

    fn transition(
        &mut self,
        hart: usize,
        from: HartState,
        to: HartState,
    ) -> Result<(), AdmissionError> {
        let state = self.lifecycle[..self.hart_count]
            .get_mut(hart)
            .ok_or(AdmissionError::InvalidHart)?;
        if state.status != from {
            return Err(AdmissionError::InvalidTransition);
        }
        state.status = to;
        Ok(())
    }
}
