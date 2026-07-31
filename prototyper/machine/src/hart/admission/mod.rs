//! Atomic admission of lifecycle transitions, IPIs, and remote fences.
//!
//! Each protocol owns a separate per-hart array. `HartAdmissionState` exists
//! only to make the acceptance of work and lifecycle gates one locked
//! linearization point; it is not a general hart context.

use super::fence::RemoteFenceRequest;
use crate::hart::{HartState, HartTargets};

#[cfg(test)]
mod audit;
mod lifecycle;
mod remote_fence;

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub(crate) struct HartSet(pub(super) u128);

impl HartSet {
    pub(crate) const fn empty() -> Self {
        Self(0)
    }

    #[cfg(test)]
    pub(crate) fn singleton(index: usize) -> Result<Self, AdmissionError> {
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

    pub(crate) fn insert(&mut self, index: usize) -> Result<(), AdmissionError> {
        let bit = 1u128
            .checked_shl(u32::try_from(index).map_err(|_| AdmissionError::InvalidHart)?)
            .ok_or(AdmissionError::InvalidHart)?;
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
pub(crate) struct ClaimedWork {
    pub(crate) supervisor_ipi: bool,
    pub(crate) sources: HartSet,
}

impl ClaimedWork {
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
struct LifecycleState {
    status: HartState,
    wake_by_ipi: bool,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct IpiState {
    pending: bool,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct FenceTargetState {
    pending_sources: HartSet,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct FenceSourceState {
    active: Option<ActiveRemoteFence>,
}

/// The narrow state covered by the work-admission lock.
///
/// Parallel arrays make ownership explicit: HSM never stores IPI or RFENCE
/// fields, and neither remote-work protocol owns lifecycle state.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct HartAdmissionState<const HARTS: usize> {
    hart_count: usize,
    physical_ids: [usize; HARTS],
    lifecycle: [LifecycleState; HARTS],
    ipi: [IpiState; HARTS],
    fence_targets: [FenceTargetState; HARTS],
    fence_sources: [FenceSourceState; HARTS],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AdmissionError {
    InvalidHart,
    Unavailable,
    SourceBusy,
    BatchBusy,
    MissingRelation,
    InvalidTransition,
    NotSupported,
}

impl<const HARTS: usize> HartAdmissionState<HARTS> {
    #[cfg(test)]
    pub(crate) fn new(
        physical_ids: [usize; HARTS],
        states: [HartState; HARTS],
        wake_by_ipi: [bool; HARTS],
    ) -> Result<Self, AdmissionError> {
        Self::new_with_count(physical_ids, states, wake_by_ipi, HARTS)
    }

    pub(crate) fn new_with_count(
        physical_ids: [usize; HARTS],
        states: [HartState; HARTS],
        wake_by_ipi: [bool; HARTS],
        hart_count: usize,
    ) -> Result<Self, AdmissionError> {
        if hart_count == 0 || hart_count > HARTS || hart_count > u128::BITS as usize {
            return Err(AdmissionError::InvalidHart);
        }
        for (index, id) in physical_ids[..hart_count].iter().enumerate() {
            if physical_ids[..index].contains(id) {
                return Err(AdmissionError::InvalidHart);
            }
        }
        Ok(Self {
            hart_count,
            physical_ids,
            lifecycle: core::array::from_fn(|index| LifecycleState {
                status: states[index],
                wake_by_ipi: wake_by_ipi[index],
            }),
            ipi: [IpiState { pending: false }; HARTS],
            fence_targets: [FenceTargetState {
                pending_sources: HartSet::empty(),
            }; HARTS],
            fence_sources: [FenceSourceState { active: None }; HARTS],
        })
    }

    pub(crate) fn state(&self, hart: usize) -> Result<HartState, AdmissionError> {
        self.lifecycle[..self.hart_count]
            .get(hart)
            .map(|state| state.status)
            .ok_or(AdmissionError::InvalidHart)
    }

    pub(crate) fn resolve_targets(&self, request: HartTargets) -> Result<HartSet, AdmissionError> {
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
                .ok_or(AdmissionError::InvalidHart)?;
            let index = self.physical_ids[..self.hart_count]
                .iter()
                .position(|id| *id == physical_id)
                .ok_or(AdmissionError::InvalidHart)?;
            if !self.serviceable(index) {
                return Err(AdmissionError::Unavailable);
            }
            targets.insert(index)?;
        }
        Ok(targets)
    }

    pub(crate) fn resolve_physical(&self, hart_id: usize) -> Result<usize, AdmissionError> {
        self.physical_ids[..self.hart_count]
            .iter()
            .position(|id| *id == hart_id)
            .ok_or(AdmissionError::InvalidHart)
    }

    pub(super) fn committed_physical_id(&self, index: usize) -> usize {
        // Protocol invariant: this is called only for a `HartSet` accepted by
        // `validate_targets` while the immutable physical-ID map remains under
        // the same admission lock. The index is therefore below
        // `hart_count`, and `hart_count` never exceeds the backing array.
        // Avoiding checked indexing here is intentional: after shared state is
        // published, device notification has no error or panic gap.
        // SAFETY: the protocol invariant above proves `index < HARTS`.
        unsafe { *self.physical_ids.get_unchecked(index) }
    }

    /// Protocol transition: publish one coalescible supervisor IPI level for
    /// the complete already-resolved target set.
    pub(crate) fn commit_ipi(&mut self, targets: HartSet) -> Result<(), AdmissionError> {
        self.validate_targets(targets)?;
        for target in targets.iter() {
            self.ipi[target].pending = true;
        }
        Ok(())
    }

    /// Protocol transition: move one complete finite pending snapshot into the
    /// target-owned batch. No locked reference escapes.
    pub(crate) fn claim(
        &mut self,
        target: usize,
        batch: &mut ClaimedWork,
    ) -> Result<(), AdmissionError> {
        if !batch.is_empty() {
            return Err(AdmissionError::BatchBusy);
        }
        let ipi = self.ipi[..self.hart_count]
            .get_mut(target)
            .ok_or(AdmissionError::InvalidHart)?;
        let fences = self.fence_targets[..self.hart_count]
            .get_mut(target)
            .ok_or(AdmissionError::InvalidHart)?;
        batch.supervisor_ipi = core::mem::take(&mut ipi.pending);
        batch.sources = core::mem::take(&mut fences.pending_sources);
        Ok(())
    }

    fn validate_targets(&self, targets: HartSet) -> Result<(), AdmissionError> {
        if !targets.within(self.hart_count) {
            return Err(AdmissionError::InvalidHart);
        }
        for target in targets.iter() {
            if !self.serviceable(target) {
                // Protocol soundness invariant: accepting work for a sleeping
                // hart without a constructed IPI wake path could strand a
                // committed remote fence forever.
                return Err(AdmissionError::Unavailable);
            }
        }
        Ok(())
    }

    fn serviceable(&self, target: usize) -> bool {
        let state = &self.lifecycle[target];
        matches!(state.status, HartState::Started | HartState::ResumePending)
            || (state.status == HartState::Suspended && state.wake_by_ipi)
    }
}
