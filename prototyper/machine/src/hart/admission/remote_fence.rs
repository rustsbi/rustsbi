//! Remote-fence source/target relation transitions.

use super::*;

impl<const HARTS: usize> HartAdmissionState<HARTS> {
    /// Atomically publishes one immutable request and every source/target
    /// relation. All target checks precede mutation.
    pub(crate) fn commit_rfence(
        &mut self,
        source: usize,
        targets: HartSet,
        request: RemoteFenceRequest,
    ) -> Result<(), AdmissionError> {
        let source_state = self.fence_sources[..self.hart_count]
            .get(source)
            .ok_or(AdmissionError::InvalidHart)?;
        if source_state.active.is_some() {
            return Err(AdmissionError::SourceBusy);
        }
        self.validate_targets(targets)?;
        if targets.is_empty() {
            return Ok(());
        }

        self.fence_sources[source].active = Some(ActiveRemoteFence {
            request,
            remaining: targets,
        });
        for target in targets.iter() {
            self.fence_targets[target].pending_sources.insert(source)?;
        }
        Ok(())
    }

    /// Copies one immutable request for execution outside the admission lock.
    pub(crate) fn copy_request(
        &self,
        target: usize,
        source: usize,
        batch: &ClaimedWork,
    ) -> Result<RemoteFenceRequest, AdmissionError> {
        if !batch.sources.contains(source) {
            return Err(AdmissionError::MissingRelation);
        }
        let active = self.fence_sources[..self.hart_count]
            .get(source)
            .ok_or(AdmissionError::InvalidHart)?
            .active
            .ok_or(AdmissionError::MissingRelation)?;
        if !active.remaining.contains(target) {
            return Err(AdmissionError::MissingRelation);
        }
        Ok(active.request)
    }

    /// Publishes completion after the target executes the architectural fence.
    pub(crate) fn complete(
        &mut self,
        target: usize,
        source: usize,
        batch: &mut ClaimedWork,
    ) -> Result<(), AdmissionError> {
        if !batch.sources.contains(source) {
            return Err(AdmissionError::MissingRelation);
        }
        let active = self.fence_sources[..self.hart_count]
            .get_mut(source)
            .ok_or(AdmissionError::InvalidHart)?
            .active
            .as_mut()
            .ok_or(AdmissionError::MissingRelation)?;
        if !active.remaining.remove(target) || !batch.sources.remove(source) {
            return Err(AdmissionError::MissingRelation);
        }
        Ok(())
    }

    /// Retires source storage only after every target published completion.
    pub(crate) fn retire(&mut self, source: usize) -> Result<RemoteFenceRequest, AdmissionError> {
        let source_state = self.fence_sources[..self.hart_count]
            .get_mut(source)
            .ok_or(AdmissionError::InvalidHart)?;
        let active = source_state.active.ok_or(AdmissionError::MissingRelation)?;
        if !active.remaining.is_empty() {
            return Err(AdmissionError::MissingRelation);
        }
        source_state.active = None;
        Ok(active.request)
    }

    pub(crate) fn ready_to_retire(&self, source: usize) -> Result<bool, AdmissionError> {
        let active = self.fence_sources[..self.hart_count]
            .get(source)
            .ok_or(AdmissionError::InvalidHart)?
            .active
            .ok_or(AdmissionError::MissingRelation)?;
        Ok(active.remaining.is_empty())
    }
}
