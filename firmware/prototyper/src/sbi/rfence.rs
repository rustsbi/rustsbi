//! Remote fence operations.
//!
//! # References
//!
//! - Specification: [RISC-V SBI RFENCE extension](https://docs.riscv.org/reference/sbi/v3.0/ext-rfence.html) —
//!   hart-mask and address-range semantics for remote fences.

use runtime::hart::HartId;
use runtime::rfence::{FenceError, FenceOperation, FenceRequest, LocalFence};
use runtime::rustsbi::{HartMask, SbiRet};
use sbi_spec::pmu::firmware_event;

use crate::cfg;

pub(super) mod queue;
use super::{hart_local, ipi, pmu};
use queue::RemoteRFenceCell;

impl RemoteRFenceCell<'_> {
    /// Adds a fence operation to the queue from a remote hart.
    fn set(&self, request: FenceRequest) {
        let hart_id = HartId::current()
            .expect("BUG: current hart exceeds Runtime capacity")
            .as_usize();
        loop {
            if self.try_push((request, hart_id)) {
                return;
            }
            rfence_poll();
        }
    }
}

/// SBI remote-fence adapter.
pub(crate) struct SbiRFence;

/// Converts a Runtime operation error to the SBI result it represents.
fn fence_error(error: FenceError) -> SbiRet {
    match error {
        FenceError::UnalignedAddress | FenceError::AddressOverflow => SbiRet::invalid_address(),
        FenceError::HypervisorUnavailable => SbiRet::not_supported(),
        #[cfg(feature = "hypervisor")]
        FenceError::GuestContextUnavailable => SbiRet::not_supported(),
        #[cfg(feature = "hypervisor")]
        FenceError::Access(_) => SbiRet::failed(),
    }
}

/// Validates and completes one remote fence batch.
fn remote_fence_process(
    operation: FenceOperation,
    start_addr: usize,
    size: usize,
    hart_mask: HartMask,
) -> SbiRet {
    let fence = LocalFence::current()
        .expect("BUG: remote fence request is outside the published hart topology");
    let request = match fence.request(operation, start_addr, size) {
        Ok(request) => request,
        Err(error) => return fence_error(error),
    };
    let current_hart = HartId::current()
        .expect("BUG: current hart is not in the boot topology")
        .as_usize();
    let requests = match ipi::target_harts(hart_mask) {
        Ok(requests) => requests,
        Err(error) => return error,
    };
    if requests
        .iter()
        .any(|hart| !rfence_target_supported(request, hart.as_usize()))
    {
        return SbiRet::not_supported();
    }
    let ipi = crate::sbi::ipi().unwrap();
    let local = hart_local::local_rfence().unwrap();
    local.begin_batch();
    let mut result = SbiRet::success(0);

    for hart in requests.iter() {
        let hart_id = hart.as_usize();
        if hart_id == current_hart {
            if let Err(error) = rfence_local_handler(request) {
                result = fence_error(error);
                break;
            }
            continue;
        }

        let remote = hart_local::remote_rfence(hart_id).unwrap();
        local.add();
        remote.set(request);

        if ipi.send_fence_ipi(hart).is_ok() {
            continue;
        }
        // Cancel this source's queued request; a receiver that already
        // took it remains responsible for the acknowledgement.
        if remote.cancel(current_hart) {
            hart_local::remote_rfence(current_hart)
                .unwrap()
                .complete(Ok(()));
        }
        result = SbiRet::failed();
        break;
    }

    // Complete previously submitted operations even if a later send failed.
    while !local.is_sync() {
        rfence_poll();
    }
    if let Some(error) = local.take_error()
        && result.is_ok()
    {
        result = fence_error(error);
    }
    result
}

/// Checks a target's advertised capabilities before queueing an operation.
fn rfence_target_supported(request: FenceRequest, hart_id: usize) -> bool {
    !request.requires_hypervisor()
        || super::features::hart_has_extension(hart_id, super::features::Extension::Hypervisor)
}

impl runtime::rustsbi::Fence for SbiRFence {
    /// Executes an instruction fence on the selected harts.
    fn remote_fence_i(&self, hart_mask: HartMask) -> SbiRet {
        pmu::pmu_firmware_counter_increment(firmware_event::FENCE_I_SENT);
        remote_fence_process(FenceOperation::FenceI, 0, 0, hart_mask)
    }

    /// Executes a supervisor virtual-memory fence on the selected harts.
    fn remote_sfence_vma(&self, hart_mask: HartMask, start_addr: usize, size: usize) -> SbiRet {
        pmu::pmu_firmware_counter_increment(firmware_event::SFENCE_VMA_SENT);
        remote_fence_process(FenceOperation::SFenceVma, start_addr, size, hart_mask)
    }

    /// Executes an ASID-specific supervisor virtual-memory fence on the selected harts.
    fn remote_sfence_vma_asid(
        &self,
        hart_mask: HartMask,
        start_addr: usize,
        size: usize,
        asid: usize,
    ) -> SbiRet {
        pmu::pmu_firmware_counter_increment(firmware_event::SFENCE_VMA_ASID_SENT);
        remote_fence_process(
            FenceOperation::SFenceVmaAsid { asid },
            start_addr,
            size,
            hart_mask,
        )
    }

    #[cfg(feature = "hypervisor")]
    fn remote_hfence_gvma_vmid(
        &self,
        hart_mask: HartMask,
        start_addr: usize,
        size: usize,
        vmid: usize,
    ) -> SbiRet {
        pmu::pmu_firmware_counter_increment(firmware_event::HFENCE_GVMA_VMID_SENT);
        remote_fence_process(
            FenceOperation::HFenceGvmaVmid { vmid },
            start_addr,
            size,
            hart_mask,
        )
    }

    #[cfg(feature = "hypervisor")]
    fn remote_hfence_gvma(&self, hart_mask: HartMask, start_addr: usize, size: usize) -> SbiRet {
        pmu::pmu_firmware_counter_increment(firmware_event::HFENCE_GVMA_SENT);
        remote_fence_process(FenceOperation::HFenceGvma, start_addr, size, hart_mask)
    }

    #[cfg(feature = "hypervisor")]
    fn remote_hfence_vvma_asid(
        &self,
        hart_mask: HartMask,
        start_addr: usize,
        size: usize,
        asid: usize,
    ) -> SbiRet {
        pmu::pmu_firmware_counter_increment(firmware_event::HFENCE_VVMA_ASID_SENT);
        remote_fence_process(
            FenceOperation::HFenceVvmaAsid { asid },
            start_addr,
            size,
            hart_mask,
        )
    }

    #[cfg(feature = "hypervisor")]
    fn remote_hfence_vvma(&self, hart_mask: HartMask, start_addr: usize, size: usize) -> SbiRet {
        pmu::pmu_firmware_counter_increment(firmware_event::HFENCE_VVMA_SENT);
        remote_fence_process(FenceOperation::HFenceVvma, start_addr, size, hart_mask)
    }
}

/// Services requests while waiting outside the IPI handler.
///
/// Senders mark FENCE pending after enqueueing. Checking the existing bit
/// avoids locking an empty queue on each synchronization retry. A stale set
/// bit only causes an unnecessary dequeue attempt. The IPI handler clears
/// this bit before draining, so it must use the unconditional handler below.
#[inline]
pub(crate) fn rfence_poll() {
    use core::sync::atomic::Ordering;

    let pending =
        hart_local::hart_local(HartId::current().expect("BUG: invalid hart ID").as_usize())
            .ipi_type
            .load(Ordering::Relaxed);
    if pending & ipi::IPI_TYPE_FENCE != 0 {
        rfence_single_handler();
    }
}

/// Handles one remote fence operation, returning whether one was dequeued.
#[inline]
fn rfence_single_handler() -> bool {
    let local_rf =
        hart_local::local_rfence().expect("BUG: RFENCE handler called outside the boot topology");

    if let Some((request, source_hart_id)) = local_rf.get() {
        // An H-capable target can still reject the caller's WARL context.
        // Acknowledge failures as well as success. Preserve receiver errors
        // for the source after all submitted requests complete.
        let result = rfence_local_handler(request);
        hart_local::remote_rfence(source_hart_id)
            .unwrap()
            .complete(result);
        true
    } else {
        false
    }
}

/// Executes a validated operation on this hart and records its received event.
#[inline]
fn rfence_local_handler(request: FenceRequest) -> Result<(), FenceError> {
    LocalFence::current()
        .expect("BUG: local fence handler is outside the published hart topology")
        .execute(request, cfg::TLB_FLUSH_LIMIT)?;
    let event = match request.operation() {
        FenceOperation::FenceI => firmware_event::FENCE_I_RECEIVED,
        FenceOperation::SFenceVma => firmware_event::SFENCE_VMA_RECEIVED,
        FenceOperation::SFenceVmaAsid { .. } => firmware_event::SFENCE_VMA_ASID_RECEIVED,
        #[cfg(feature = "hypervisor")]
        FenceOperation::HFenceGvmaVmid { .. } => firmware_event::HFENCE_GVMA_VMID_RECEIVED,
        #[cfg(feature = "hypervisor")]
        FenceOperation::HFenceGvma => firmware_event::HFENCE_GVMA_RECEIVED,
        #[cfg(feature = "hypervisor")]
        FenceOperation::HFenceVvmaAsid { .. } => firmware_event::HFENCE_VVMA_ASID_RECEIVED,
        #[cfg(feature = "hypervisor")]
        FenceOperation::HFenceVvma => firmware_event::HFENCE_VVMA_RECEIVED,
    };
    pmu::pmu_firmware_counter_increment(event);
    Ok(())
}

/// Processes all pending remote fence operations on the current hart.
#[inline]
pub(super) fn rfence_handler() {
    while rfence_single_handler() {}
}
