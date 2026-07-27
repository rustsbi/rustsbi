//! Shared work-admission boundary for hart protocols.

use alloc::boxed::Box;
use alloc::sync::Arc;
use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicUsize, Ordering};

use super::admission::*;
use super::instructions::{current_hart_id, execute, manifest_supervisor_ipi, protocol_fence};
use super::ipi::{IpiDevice, IpiError};
use super::lock::TicketLock;
use super::start::PendingStart;
use crate::config::HART_CAPACITY;
use crate::hart::{HartError, HartStatus};
/// Narrow atomic boundary shared by HSM, IPI, and RFENCE.
///
/// The object owns no combined per-hart record. It serializes only operations
/// whose acceptance must be ordered against stop and suspend gates.
pub(crate) struct HartAdmission {
    pub(super) state: TicketLock<AdmissionState>,
    pub(super) device: Arc<dyn IpiDevice>,
    physical_ids: Box<[usize]>,
}

const RUNTIME_EMPTY: usize = 0;
const RUNTIME_WRITING: usize = 1;
const RUNTIME_READY: usize = 2;

struct PublishedHartAdmission {
    state: AtomicUsize,
    value: UnsafeCell<Option<Arc<HartAdmission>>>,
}

// SAFETY: only the successful empty-to-writing claimant initializes `value`.
// It is immutable after Release publication and all readers first Acquire the
// ready state.
unsafe impl Sync for PublishedHartAdmission {}

static ADMISSION: PublishedHartAdmission = PublishedHartAdmission {
    state: AtomicUsize::new(RUNTIME_EMPTY),
    value: UnsafeCell::new(None),
};

pub(crate) fn publish(admission: Arc<HartAdmission>) -> Result<(), IpiError> {
    ADMISSION
        .state
        .compare_exchange(
            RUNTIME_EMPTY,
            RUNTIME_WRITING,
            Ordering::Acquire,
            Ordering::Relaxed,
        )
        .map_err(|_| IpiError::Failed)?;
    // SAFETY: this caller uniquely owns the writing state, and readers cannot
    // inspect the field until the Release publication below.
    unsafe { ADMISSION.value.get().write(Some(admission)) };
    ADMISSION.state.store(RUNTIME_READY, Ordering::Release);
    Ok(())
}

pub(crate) fn installed() -> Option<&'static HartAdmission> {
    if ADMISSION.state.load(Ordering::Acquire) != RUNTIME_READY {
        return None;
    }
    // SAFETY: the Acquire load observed the sole Release publication. The Arc
    // remains stored and immutable for the firmware lifetime.
    unsafe { (&*ADMISSION.value.get()).as_deref() }
}

pub(crate) fn notify_terminal_peers() {
    if let Some(admission) = installed() {
        admission.notify_terminal_peers();
    }
}

pub(super) struct AdmissionState {
    pub(super) work: HartAdmissionState<HART_CAPACITY>,
    pub(super) starts: [Option<PendingStart>; HART_CAPACITY],
}

impl Deref for AdmissionState {
    type Target = HartAdmissionState<HART_CAPACITY>;

    fn deref(&self) -> &Self::Target {
        &self.work
    }
}

impl DerefMut for AdmissionState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.work
    }
}

impl HartAdmission {
    pub(crate) fn new(
        device: Arc<dyn IpiDevice>,
        physical_ids: &[usize],
        boot_hart: usize,
        wake_by_ipi: &[bool],
    ) -> Result<Arc<Self>, IpiError> {
        if physical_ids.is_empty()
            || physical_ids.len() > HART_CAPACITY
            || physical_ids.len() != wake_by_ipi.len()
        {
            return Err(IpiError::InvalidHart);
        }
        let mut ids = [0usize; HART_CAPACITY];
        let mut states = [HartStatus::Stopped; HART_CAPACITY];
        let mut wake = [false; HART_CAPACITY];
        ids[..physical_ids.len()].copy_from_slice(physical_ids);
        wake[..wake_by_ipi.len()].copy_from_slice(wake_by_ipi);
        let boot_index = physical_ids
            .iter()
            .position(|hart| *hart == boot_hart)
            .ok_or(IpiError::InvalidHart)?;
        states[boot_index] = HartStatus::Started;
        let state = HartAdmissionState::new_with_count(ids, states, wake, physical_ids.len())
            .map_err(map_ipi_error)?;
        Ok(Arc::new(Self {
            state: TicketLock::new(AdmissionState {
                work: state,
                starts: core::array::from_fn(|_| None),
            }),
            device,
            physical_ids: physical_ids.to_vec().into_boxed_slice(),
        }))
    }

    /// Confirms that device construction and architectural discovery admitted
    /// the same ordered physical-hart set.
    pub(crate) fn matches_harts(&self, physical_ids: &[usize]) -> bool {
        self.physical_ids.as_ref() == physical_ids
    }

    fn notify_terminal_peers(&self) {
        let current = current_hart_id();
        // Architecture invariant: these IDs are the immutable admitted set,
        // and the device fence orders fatal-state publication before every
        // best-effort emergency ring. No ordinary protocol lock is acquired.
        protocol_fence();
        for &physical in self.physical_ids.iter().filter(|id| **id != current) {
            self.device.notify(physical);
        }
        protocol_fence();
    }

    pub(super) fn notify(&self, notifications: HartNotifications) {
        protocol_fence();
        for physical in notifications.iter() {
            self.device.notify(physical);
        }
        protocol_fence();
    }

    pub(super) fn drain(
        &self,
        physical_hart: usize,
        claim_device: bool,
    ) -> Result<HartDrainOutcome, IpiError> {
        if claim_device {
            self.device.claim(physical_hart);
            protocol_fence();
        }
        let (target, mut batch) = {
            let mut state = self.state.lock();
            let target = state
                .work
                .resolve_physical(physical_hart)
                .map_err(map_ipi_error)?;
            let mut batch = ClaimedWork::default();
            state
                .work
                .claim(target, &mut batch)
                .map_err(map_ipi_error)?;
            (target, batch)
        };
        let supervisor_interrupt = batch.supervisor_ipi;
        if supervisor_interrupt {
            manifest_supervisor_ipi();
            batch.supervisor_ipi = false;
        }
        while let Some(source) = batch.sources.iter().next() {
            let request = {
                let state = self.state.lock();
                state
                    .work
                    .copy_request(target, source, &batch)
                    .map_err(map_ipi_error)?
            };
            execute(request);
            let mut state = self.state.lock();
            state
                .work
                .complete(target, source, &mut batch)
                .map_err(map_ipi_error)?;
        }
        Ok(HartDrainOutcome {
            supervisor_interrupt,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct HartDrainOutcome {
    pub(super) supervisor_interrupt: bool,
}

#[derive(Clone, Copy)]
pub(super) struct HartNotifications {
    physical_harts: [usize; HART_CAPACITY],
    len: usize,
}

impl HartNotifications {
    pub(super) fn from_state(
        state: &HartAdmissionState<HART_CAPACITY>,
        source: usize,
        targets: HartSet,
    ) -> Self {
        let mut physical_harts = [0; HART_CAPACITY];
        let mut len = 0;
        for target in targets.iter().filter(|target| *target != source) {
            physical_harts[len] = state.committed_physical_id(target);
            len += 1;
        }
        Self {
            physical_harts,
            len,
        }
    }

    fn iter(self) -> impl Iterator<Item = usize> {
        self.physical_harts.into_iter().take(self.len)
    }
}

pub(super) fn map_ipi_error(error: AdmissionError) -> IpiError {
    match error {
        AdmissionError::InvalidHart | AdmissionError::Unavailable => IpiError::InvalidHart,
        AdmissionError::SourceBusy
        | AdmissionError::BatchBusy
        | AdmissionError::MissingRelation
        | AdmissionError::InvalidTransition
        | AdmissionError::NotSupported => IpiError::Failed,
    }
}

pub(super) fn map_hart_error(error: AdmissionError) -> HartError {
    match error {
        AdmissionError::InvalidHart | AdmissionError::Unavailable => HartError::InvalidHart,
        AdmissionError::SourceBusy
        | AdmissionError::BatchBusy
        | AdmissionError::MissingRelation
        | AdmissionError::InvalidTransition
        | AdmissionError::NotSupported => HartError::Failed,
    }
}

#[cfg(test)]
mod tests;
