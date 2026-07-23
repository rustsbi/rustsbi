//! Remote architectural-fence capability.

use alloc::sync::Arc;

use super::admission::{AdmissionError, HartSet};
use super::arch::current_hart_id;
use super::protocol::{HartAdmission, HartNotifications};
use crate::hart::HartTargets;

/// One immutable architectural fence copied to every selected hart.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum RemoteFenceRequest {
    FenceI,
    SfenceVma {
        start: usize,
        size: usize,
    },
    SfenceVmaAsid {
        start: usize,
        size: usize,
        asid: usize,
    },
    #[cfg(feature = "hypervisor")]
    HfenceGvma {
        start: usize,
        size: usize,
    },
    #[cfg(feature = "hypervisor")]
    HfenceGvmaVmid {
        start: usize,
        size: usize,
        vmid: usize,
    },
    #[cfg(feature = "hypervisor")]
    HfenceVvma {
        start: usize,
        size: usize,
    },
    #[cfg(feature = "hypervisor")]
    HfenceVvmaAsid {
        start: usize,
        size: usize,
        asid: usize,
    },
}

impl RemoteFenceRequest {
    fn requires_hypervisor(self) -> bool {
        #[cfg(feature = "hypervisor")]
        if matches!(
            self,
            Self::HfenceGvma { .. }
                | Self::HfenceGvmaVmid { .. }
                | Self::HfenceVvma { .. }
                | Self::HfenceVvmaAsid { .. }
        ) {
            return true;
        }
        false
    }
}

pub(super) fn targets_support_request(
    request: RemoteFenceRequest,
    targets: HartSet,
    mut hypervisor_available: impl FnMut(usize) -> bool,
) -> bool {
    !request.requires_hypervisor() || targets.iter().all(&mut hypervisor_available)
}

/// Failure from a remote fence request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RemoteFenceError {
    /// A selected architectural hart does not exist or is unavailable.
    InvalidHart,
    /// The requested virtual-address range wraps the address space.
    InvalidAddress,
    /// The requested architectural fence is unavailable in this build.
    NotSupported,
    /// The shared hart-state protocol detected an internal failure.
    Failed,
}

/// Authority to execute architectural fences on admitted remote harts.
pub struct RemoteFence {
    admission: Arc<HartAdmission>,
}

impl RemoteFence {
    pub(crate) fn new(admission: Arc<HartAdmission>) -> Self {
        Self { admission }
    }

    /// Synchronizes instruction fetch with prior stores on every target.
    pub fn fence_i(&self, targets: HartTargets) -> Result<(), RemoteFenceError> {
        self.execute(targets, RemoteFenceRequest::FenceI)
    }

    /// Invalidates supervisor translations in the requested address range.
    pub fn sfence_vma(
        &self,
        targets: HartTargets,
        start_addr: usize,
        size: usize,
    ) -> Result<(), RemoteFenceError> {
        validate_range(start_addr, size)?;
        self.execute(
            targets,
            RemoteFenceRequest::SfenceVma {
                start: start_addr,
                size,
            },
        )
    }

    /// Invalidates supervisor translations for one ASID and address range.
    pub fn sfence_vma_asid(
        &self,
        targets: HartTargets,
        start_addr: usize,
        size: usize,
        asid: usize,
    ) -> Result<(), RemoteFenceError> {
        validate_range(start_addr, size)?;
        self.execute(
            targets,
            RemoteFenceRequest::SfenceVmaAsid {
                start: start_addr,
                size,
                asid,
            },
        )
    }

    /// Invalidates guest-physical translations in the requested range.
    #[cfg(feature = "hypervisor")]
    pub fn hfence_gvma(
        &self,
        targets: HartTargets,
        start_addr: usize,
        size: usize,
    ) -> Result<(), RemoteFenceError> {
        validate_range(start_addr, size)?;
        self.execute(
            targets,
            RemoteFenceRequest::HfenceGvma {
                start: start_addr,
                size,
            },
        )
    }

    /// Invalidates guest-physical translations for one VMID.
    #[cfg(feature = "hypervisor")]
    pub fn hfence_gvma_vmid(
        &self,
        targets: HartTargets,
        start_addr: usize,
        size: usize,
        vmid: usize,
    ) -> Result<(), RemoteFenceError> {
        validate_range(start_addr, size)?;
        self.execute(
            targets,
            RemoteFenceRequest::HfenceGvmaVmid {
                start: start_addr,
                size,
                vmid,
            },
        )
    }

    /// Invalidates guest-virtual translations in the requested range.
    #[cfg(feature = "hypervisor")]
    pub fn hfence_vvma(
        &self,
        targets: HartTargets,
        start_addr: usize,
        size: usize,
    ) -> Result<(), RemoteFenceError> {
        validate_range(start_addr, size)?;
        self.execute(
            targets,
            RemoteFenceRequest::HfenceVvma {
                start: start_addr,
                size,
            },
        )
    }

    /// Invalidates guest-virtual translations for one ASID.
    #[cfg(feature = "hypervisor")]
    pub fn hfence_vvma_asid(
        &self,
        targets: HartTargets,
        start_addr: usize,
        size: usize,
        asid: usize,
    ) -> Result<(), RemoteFenceError> {
        validate_range(start_addr, size)?;
        self.execute(
            targets,
            RemoteFenceRequest::HfenceVvmaAsid {
                start: start_addr,
                size,
                asid,
            },
        )
    }

    fn execute(
        &self,
        targets: HartTargets,
        request: RemoteFenceRequest,
    ) -> Result<(), RemoteFenceError> {
        self.admission
            .remote_fence(targets, request)
            .map_err(map_error)
    }
}

impl HartAdmission {
    /// Commits one remote fence and waits until every target has executed it.
    pub(crate) fn remote_fence(
        &self,
        targets: HartTargets,
        request: RemoteFenceRequest,
    ) -> Result<(), AdmissionError> {
        let current_hart = current_hart_id();
        let (source, resolved, notifications) = {
            let mut state = self.state.lock();
            let source = state.resolve_physical(current_hart)?;
            let resolved = state.resolve_targets(targets)?;
            if !targets_support_request(request, resolved, crate::trap::hypervisor_available) {
                return Err(AdmissionError::NotSupported);
            }
            state.commit_rfence(source, resolved, request)?;
            let notifications = HartNotifications::from_state(&state, source, resolved);
            (source, resolved, notifications)
        };
        self.notify(notifications);
        if resolved.is_empty() {
            return Ok(());
        }

        loop {
            self.drain(current_hart, false)
                .map_err(|_| AdmissionError::InvalidHart)?;
            let mut state = self.state.lock();
            if state.ready_to_retire(source)? {
                state.retire(source)?;
                return Ok(());
            }
            drop(state);
            core::hint::spin_loop();
        }
    }
}

fn validate_range(start: usize, size: usize) -> Result<(), RemoteFenceError> {
    if size != 0 && start.checked_add(size - 1).is_none() {
        return Err(RemoteFenceError::InvalidAddress);
    }
    Ok(())
}

fn map_error(error: AdmissionError) -> RemoteFenceError {
    match error {
        AdmissionError::InvalidHart | AdmissionError::Unavailable => RemoteFenceError::InvalidHart,
        AdmissionError::NotSupported => RemoteFenceError::NotSupported,
        AdmissionError::SourceBusy
        | AdmissionError::BatchBusy
        | AdmissionError::MissingRelation
        | AdmissionError::InvalidTransition => RemoteFenceError::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_validation_accepts_empty_and_last_byte_ranges() {
        assert_eq!(validate_range(usize::MAX, 0), Ok(()));
        assert_eq!(validate_range(usize::MAX, 1), Ok(()));
        assert_eq!(
            validate_range(usize::MAX, 2),
            Err(RemoteFenceError::InvalidAddress)
        );
    }
}
