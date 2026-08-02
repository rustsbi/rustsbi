//! Model invariants and test-only observations.

use super::*;

impl<const HARTS: usize> HartAdmissionState<HARTS> {
    pub(crate) fn invariants_hold(&self, batches: &[ClaimedWork; HARTS]) -> bool {
        for target in 0..self.hart_count {
            if !self.fence_targets[target]
                .pending_sources
                .within(self.hart_count)
                || !batches[target].sources.within(self.hart_count)
            {
                return false;
            }
            if !self.fence_targets[target]
                .pending_sources
                .is_disjoint(batches[target].sources)
            {
                return false;
            }
            if self.lifecycle[target].status == HartState::Stopped
                && (self.ipi[target].pending
                    || !self.fence_targets[target].pending_sources.is_empty()
                    || !batches[target].is_empty()
                    || self.fence_sources[target].active.is_some())
            {
                return false;
            }
        }

        for source in 0..self.hart_count {
            let active = self.fence_sources[source].active;
            for (target, batch) in batches[..self.hart_count].iter().enumerate() {
                let pending = self.fence_targets[target].pending_sources.contains(source);
                let claimed = batch.sources.contains(source);
                let outstanding = active.is_some_and(|active| active.remaining.contains(target));
                if outstanding != (pending ^ claimed) || (pending || claimed) && !outstanding {
                    return false;
                }
            }
        }
        true
    }

    #[cfg(test)]
    pub(crate) fn ipi_pending(&self, target: usize) -> bool {
        self.ipi.get(target).is_some_and(|state| state.pending)
    }

    #[cfg(test)]
    pub(crate) fn fence_pending(&self, target: usize, source: usize) -> bool {
        self.fence_targets
            .get(target)
            .is_some_and(|state| state.pending_sources.contains(source))
    }

    #[cfg(test)]
    pub(crate) fn fence_source_idle(&self, source: usize) -> bool {
        self.fence_sources
            .get(source)
            .is_some_and(|state| state.active.is_none())
    }

    #[cfg(test)]
    pub(crate) fn all_fence_sources_idle(&self) -> bool {
        self.fence_sources[..self.hart_count]
            .iter()
            .all(|state| state.active.is_none())
    }
}
