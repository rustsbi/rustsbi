//! Pure locked state transitions for hart lifecycle and pending work.

use super::fence::RemoteFenceRequest;
use crate::hart::{HartStatus, HartTargets};

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub(crate) struct HartSet(pub(super) u128);

impl HartSet {
    pub(crate) const fn empty() -> Self {
        Self(0)
    }

    #[cfg(test)]
    pub(crate) fn singleton(index: usize) -> Result<Self, HartStateError> {
        let mut set = Self::empty();
        set.insert(index)?;
        Ok(set)
    }

    pub(crate) fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub(crate) fn contains(self, index: usize) -> bool {
        index < u128::BITS as usize && self.0 & (1u128 << index) != 0
    }

    pub(crate) fn insert(&mut self, index: usize) -> Result<(), HartStateError> {
        let bit = 1u128
            .checked_shl(u32::try_from(index).map_err(|_| HartStateError::InvalidHart)?)
            .ok_or(HartStateError::InvalidHart)?;
        self.0 |= bit;
        Ok(())
    }

    pub(crate) fn remove(&mut self, index: usize) -> bool {
        if !self.contains(index) {
            return false;
        }
        self.0 &= !(1u128 << index);
        true
    }

    pub(crate) fn iter(self) -> HartSetIter {
        HartSetIter(self.0)
    }

    fn within(self, hart_count: usize) -> bool {
        hart_count <= u128::BITS as usize
            && (hart_count == u128::BITS as usize || self.0 >> hart_count == 0)
    }
}

pub(crate) struct HartSetIter(u128);

impl Iterator for HartSetIter {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        if self.0 == 0 {
            return None;
        }
        let index = self.0.trailing_zeros() as usize;
        self.0 &= self.0 - 1;
        Some(index)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub(crate) struct PendingHartWork {
    pub(crate) supervisor_ipi: bool,
    pub(crate) sources: HartSet,
}

impl PendingHartWork {
    pub(crate) fn is_empty(self) -> bool {
        !self.supervisor_ipi && self.sources.is_empty()
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct ActiveRemoteFence {
    request: RemoteFenceRequest,
    remaining: HartSet,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct HartEntry {
    pub(super) hsm: HartStatus,
    pub(super) wake_by_ipi: bool,
    pub(super) supervisor_ipi: bool,
    pub(super) pending_sources: HartSet,
    pub(super) active: Option<ActiveRemoteFence>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct HartState<const HARTS: usize> {
    hart_count: usize,
    physical_ids: [usize; HARTS],
    pub(super) harts: [HartEntry; HARTS],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HartStateError {
    InvalidHart,
    Unavailable,
    SourceBusy,
    BatchBusy,
    MissingRelation,
    InvalidTransition,
    NotSupported,
}

impl<const HARTS: usize> HartState<HARTS> {
    #[cfg(test)]
    pub(crate) fn new(
        physical_ids: [usize; HARTS],
        states: [HartStatus; HARTS],
        wake_by_ipi: [bool; HARTS],
    ) -> Result<Self, HartStateError> {
        Self::new_with_count(physical_ids, states, wake_by_ipi, HARTS)
    }

    pub(crate) fn new_with_count(
        physical_ids: [usize; HARTS],
        states: [HartStatus; HARTS],
        wake_by_ipi: [bool; HARTS],
        hart_count: usize,
    ) -> Result<Self, HartStateError> {
        if hart_count == 0 || hart_count > HARTS || hart_count > u128::BITS as usize {
            return Err(HartStateError::InvalidHart);
        }
        for (index, id) in physical_ids[..hart_count].iter().enumerate() {
            if physical_ids[..index].contains(id) {
                return Err(HartStateError::InvalidHart);
            }
        }
        Ok(Self {
            hart_count,
            physical_ids,
            harts: core::array::from_fn(|index| HartEntry {
                hsm: states[index],
                wake_by_ipi: wake_by_ipi[index],
                supervisor_ipi: false,
                pending_sources: HartSet::empty(),
                active: None,
            }),
        })
    }

    pub(crate) fn state(&self, hart: usize) -> Result<HartStatus, HartStateError> {
        self.harts[..self.hart_count]
            .get(hart)
            .map(|record| record.hsm)
            .ok_or(HartStateError::InvalidHart)
    }

    pub(crate) fn resolve_targets(&self, request: HartTargets) -> Result<HartSet, HartStateError> {
        let Some((mut bits, base)) = request.selected_parts() else {
            let mut targets = HartSet::empty();
            for index in 0..self.hart_count {
                if self.serviceable(index) {
                    targets.insert(index)?;
                }
            }
            return Ok(targets);
        };

        let mut targets = HartSet::empty();
        while bits != 0 {
            let offset = bits.trailing_zeros() as usize;
            bits &= bits - 1;
            let physical_id = base
                .checked_add(offset)
                .ok_or(HartStateError::InvalidHart)?;
            let index = self.physical_ids[..self.hart_count]
                .iter()
                .position(|id| *id == physical_id)
                .ok_or(HartStateError::InvalidHart)?;
            if !self.serviceable(index) {
                return Err(HartStateError::Unavailable);
            }
            targets.insert(index)?;
        }
        Ok(targets)
    }

    pub(crate) fn resolve_physical(&self, hart_id: usize) -> Result<usize, HartStateError> {
        self.physical_ids[..self.hart_count]
            .iter()
            .position(|id| *id == hart_id)
            .ok_or(HartStateError::InvalidHart)
    }

    pub(super) fn committed_physical_id(&self, index: usize) -> usize {
        // Protocol invariant: this is called only for a `HartSet` accepted by
        // `validate_targets` while the immutable physical-ID map remains under
        // the same runtime lock. The index is therefore below
        // `hart_count`, and `hart_count` never exceeds the backing array.
        // Avoiding checked indexing here is intentional: after shared state is
        // published, device notification has no error or panic gap.
        // SAFETY: the protocol invariant above proves `index < HARTS`.
        unsafe { *self.physical_ids.get_unchecked(index) }
    }

    /// Protocol transition: publish one coalescible supervisor IPI level for
    /// the complete already-resolved target set.
    pub(crate) fn commit_ipi(&mut self, targets: HartSet) -> Result<(), HartStateError> {
        self.validate_targets(targets)?;
        for target in targets.iter() {
            self.harts[target].supervisor_ipi = true;
        }
        Ok(())
    }

    /// Protocol transition: atomically publish one immutable request and its
    /// complete source/target relation. All target checks precede mutation.
    pub(crate) fn commit_rfence(
        &mut self,
        source: usize,
        targets: HartSet,
        request: RemoteFenceRequest,
    ) -> Result<(), HartStateError> {
        let source_record = self.harts[..self.hart_count]
            .get(source)
            .ok_or(HartStateError::InvalidHart)?;
        if source_record.active.is_some() {
            return Err(HartStateError::SourceBusy);
        }
        self.validate_targets(targets)?;
        if targets.is_empty() {
            return Ok(());
        }

        self.harts[source].active = Some(ActiveRemoteFence {
            request,
            remaining: targets,
        });
        for target in targets.iter() {
            self.harts[target].pending_sources.insert(source)?;
        }
        Ok(())
    }

    /// Protocol transition: move one complete finite pending snapshot into the
    /// target-owned batch. No locked reference escapes.
    pub(crate) fn claim(
        &mut self,
        target: usize,
        batch: &mut PendingHartWork,
    ) -> Result<(), HartStateError> {
        if !batch.is_empty() {
            return Err(HartStateError::BatchBusy);
        }
        let record = self.harts[..self.hart_count]
            .get_mut(target)
            .ok_or(HartStateError::InvalidHart)?;
        batch.supervisor_ipi = core::mem::take(&mut record.supervisor_ipi);
        batch.sources = core::mem::take(&mut record.pending_sources);
        Ok(())
    }

    /// Protocol transition: copy the immutable source request for execution
    /// outside the runtime lock.
    pub(crate) fn copy_request(
        &self,
        target: usize,
        source: usize,
        batch: &PendingHartWork,
    ) -> Result<RemoteFenceRequest, HartStateError> {
        if !batch.sources.contains(source) {
            return Err(HartStateError::MissingRelation);
        }
        let active = self.harts[..self.hart_count]
            .get(source)
            .ok_or(HartStateError::InvalidHart)?
            .active
            .ok_or(HartStateError::MissingRelation)?;
        if !active.remaining.contains(target) {
            return Err(HartStateError::MissingRelation);
        }
        Ok(active.request)
    }

    /// Protocol transition: publish completion for exactly one source/target
    /// relation after the target executed the complete architectural fence.
    pub(crate) fn complete(
        &mut self,
        target: usize,
        source: usize,
        batch: &mut PendingHartWork,
    ) -> Result<(), HartStateError> {
        if !batch.sources.contains(source) {
            return Err(HartStateError::MissingRelation);
        }
        let active = self.harts[..self.hart_count]
            .get_mut(source)
            .ok_or(HartStateError::InvalidHart)?
            .active
            .as_mut()
            .ok_or(HartStateError::MissingRelation)?;
        if !active.remaining.remove(target) || !batch.sources.remove(source) {
            return Err(HartStateError::MissingRelation);
        }
        Ok(())
    }

    /// Protocol transition: retire one request only after all targets have
    /// published completion, making source storage reusable.
    pub(crate) fn retire(&mut self, source: usize) -> Result<RemoteFenceRequest, HartStateError> {
        let record = self.harts[..self.hart_count]
            .get_mut(source)
            .ok_or(HartStateError::InvalidHart)?;
        let active = record.active.ok_or(HartStateError::MissingRelation)?;
        if !active.remaining.is_empty() {
            return Err(HartStateError::MissingRelation);
        }
        record.active = None;
        Ok(active.request)
    }

    pub(crate) fn ready_to_retire(&self, source: usize) -> Result<bool, HartStateError> {
        let active = self.harts[..self.hart_count]
            .get(source)
            .ok_or(HartStateError::InvalidHart)?
            .active
            .ok_or(HartStateError::MissingRelation)?;
        Ok(active.remaining.is_empty())
    }

    pub(crate) fn begin_start(&mut self, target: usize) -> Result<(), HartStateError> {
        self.transition(target, HartStatus::Stopped, HartStatus::StartPending)
    }

    pub(crate) fn complete_start(&mut self, target: usize) -> Result<(), HartStateError> {
        self.transition(target, HartStatus::StartPending, HartStatus::Started)
    }

    pub(crate) fn cancel_start(&mut self, target: usize) -> Result<(), HartStateError> {
        self.transition(target, HartStatus::StartPending, HartStatus::Stopped)
    }

    pub(crate) fn begin_stop(&mut self, target: usize) -> Result<(), HartStateError> {
        self.transition(target, HartStatus::Started, HartStatus::StopPending)
    }

    pub(crate) fn finish_stop(
        &mut self,
        target: usize,
        batch: &PendingHartWork,
    ) -> Result<(), HartStateError> {
        if !batch.is_empty() {
            return Err(HartStateError::BatchBusy);
        }
        let record = self.harts[..self.hart_count]
            .get(target)
            .ok_or(HartStateError::InvalidHart)?;
        if record.supervisor_ipi || !record.pending_sources.is_empty() || record.active.is_some() {
            return Err(HartStateError::MissingRelation);
        }
        self.transition(target, HartStatus::StopPending, HartStatus::Stopped)
    }

    pub(crate) fn begin_suspend(&mut self, target: usize) -> Result<(), HartStateError> {
        self.transition(target, HartStatus::Started, HartStatus::SuspendPending)
    }

    /// Begins the system-wide suspend transition only while every peer hart is
    /// stopped. The check and transition occur under the runtime's single
    /// protocol lock, so a concurrent hart-start request cannot pass between
    /// them.
    pub(crate) fn begin_system_suspend(&mut self, target: usize) -> Result<(), HartStateError> {
        if target >= self.hart_count {
            return Err(HartStateError::InvalidHart);
        }
        if self.harts[..self.hart_count]
            .iter()
            .enumerate()
            .any(|(index, record)| index != target && record.hsm != HartStatus::Stopped)
        {
            return Err(HartStateError::Unavailable);
        }
        self.begin_suspend(target)
    }

    pub(crate) fn finish_suspend(&mut self, target: usize) -> Result<(), HartStateError> {
        let record = self.harts[..self.hart_count]
            .get(target)
            .ok_or(HartStateError::InvalidHart)?;
        if record.supervisor_ipi || !record.pending_sources.is_empty() || record.active.is_some() {
            return Err(HartStateError::MissingRelation);
        }
        self.transition(target, HartStatus::SuspendPending, HartStatus::Suspended)
    }

    pub(super) fn wakeable_by_ipi(&self, target: usize) -> Result<bool, HartStateError> {
        self.harts[..self.hart_count]
            .get(target)
            .map(|record| record.wake_by_ipi)
            .ok_or(HartStateError::InvalidHart)
    }

    pub(crate) fn begin_resume(&mut self, target: usize) -> Result<(), HartStateError> {
        self.transition(target, HartStatus::Suspended, HartStatus::ResumePending)
    }

    pub(crate) fn finish_resume(&mut self, target: usize) -> Result<(), HartStateError> {
        self.transition(target, HartStatus::ResumePending, HartStatus::Started)
    }

    #[cfg(test)]
    pub(crate) fn invariants_hold(&self, batches: &[PendingHartWork; HARTS]) -> bool {
        for (target, record) in self.harts[..self.hart_count].iter().enumerate() {
            if !record.pending_sources.within(self.hart_count)
                || !batches[target].sources.within(self.hart_count)
            {
                return false;
            }
            if !(record.pending_sources.0 & batches[target].sources.0 == 0) {
                return false;
            }
            if record.hsm == HartStatus::Stopped
                && (record.supervisor_ipi
                    || !record.pending_sources.is_empty()
                    || !batches[target].is_empty()
                    || record.active.is_some())
            {
                return false;
            }
        }

        for source in 0..self.hart_count {
            let active = self.harts[source].active;
            for (target, batch) in batches[..self.hart_count].iter().enumerate() {
                let pending = self.harts[target].pending_sources.contains(source);
                let claimed = batch.sources.contains(source);
                let outstanding = active.is_some_and(|active| active.remaining.contains(target));
                if outstanding != (pending ^ claimed) {
                    return false;
                }
                if (pending || claimed) && !outstanding {
                    return false;
                }
            }
        }
        true
    }

    fn validate_targets(&self, targets: HartSet) -> Result<(), HartStateError> {
        if !targets.within(self.hart_count) {
            return Err(HartStateError::InvalidHart);
        }
        for target in targets.iter() {
            if !self.serviceable(target) {
                // Protocol soundness invariant: accepting work for a sleeping
                // hart without a constructed IPI wake path could strand a
                // committed remote fence forever.
                return Err(HartStateError::Unavailable);
            }
        }
        Ok(())
    }

    fn serviceable(&self, target: usize) -> bool {
        let record = &self.harts[target];
        matches!(record.hsm, HartStatus::Started | HartStatus::ResumePending)
            || (record.hsm == HartStatus::Suspended && record.wake_by_ipi)
    }

    fn transition(
        &mut self,
        hart: usize,
        from: HartStatus,
        to: HartStatus,
    ) -> Result<(), HartStateError> {
        let record = self.harts[..self.hart_count]
            .get_mut(hart)
            .ok_or(HartStateError::InvalidHart)?;
        if record.hsm != from {
            return Err(HartStateError::InvalidTransition);
        }
        record.hsm = to;
        Ok(())
    }
}
