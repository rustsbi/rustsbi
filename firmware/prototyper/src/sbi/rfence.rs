//! Remote fence operations.
//!
//! # References
//!
//! - Specification: [RISC-V SBI RFENCE extension](https://docs.riscv.org/reference/sbi/v3.0/ext-rfence.html) —
//!   hart-mask and address-range semantics for remote fences.
//! - Specification: [RISC-V hypervisor extension](https://docs.riscv.org/reference/isa/priv/hypervisor.html) —
//!   hypervisor fence instructions and their execution requirements.

#![forbid(unsafe_code)]

use runtime::{
    Error, Result,
    hart::HartId,
    ipi::{self, IpiEvent},
    rfence::{FenceOperation, FenceRequest, LocalFence},
    rustsbi::{Fence, HartMask, SbiRet},
};
use sbi_spec::pmu::firmware_event;

use crate::cfg::TLB_FLUSH_LIMIT;

pub(super) mod queue;
use super::{ipi::target_harts, pmu::pmu_firmware_counter_increment};
use queue::{LocalRFenceCell, RemoteRFenceCell};

/// Creates one fence queue and synchronization counter per enabled hart.
///
/// The caller must run this after Runtime publishes the hart topology and
/// before any hart enqueues or services a remote fence.
pub(crate) fn init() {
    queue::init();
}

/// Returns a local view of the fence state for a validated hart.
pub(crate) fn local_rfence_for(hart: HartId) -> LocalRFenceCell<'static> {
    queue::rfence_cell(hart).local()
}

/// Returns a remote view of the fence state for a validated hart.
pub(crate) fn remote_rfence_for(hart: HartId) -> RemoteRFenceCell<'static> {
    queue::rfence_cell(hart).remote()
}

impl RemoteRFenceCell<'_> {
    /// Adds a fence operation to the queue from a remote hart.
    ///
    /// Retries while the queue is full or busy.
    pub fn set(&self, request: FenceRequest) {
        let hart = HartId::current().expect("BUG: current hart exceeds Runtime capacity");
        loop {
            if self.try_push((request, hart)) {
                return;
            }
            rfence_poll_for(hart);
        }
    }
}

/// Implementation of RISC-V remote fence operations.
pub(crate) struct SbiRFence;

impl SbiRFence {
    pub(crate) fn new() -> Result<Self> {
        ipi::get()
            .ok_or(Error::NotReady)?
            .register(IpiEvent::Rfence, rfence_handler)?;
        Ok(Self)
    }
}

/// Processes a remote fence operation by sending IPI to target harts.
fn remote_fence_process(request: FenceRequest, hart_mask: HartMask) -> SbiRet {
    let current_hart = HartId::current().expect("BUG: current hart is not in the boot topology");
    // Reject currently available targets lacking H before publishing work.
    // Availability can change before delivery; each Runtime executor also
    // checks its actual H support and returns errors through completion.
    if request.requires_hypervisor() {
        let supported = match target_harts(hart_mask) {
            Ok(mut targets) => targets.all(|hart| {
                super::features::has_extension(hart, super::features::Extension::Hypervisor)
            }),
            Err(error) => return error,
        };
        if !supported {
            return SbiRet::not_supported();
        }
    }
    let targets = match target_harts(hart_mask) {
        Ok(targets) => targets,
        Err(error) => return error,
    };
    let local = local_rfence_for(current_hart);
    let mut result = SbiRet::success(0);

    for target in targets {
        if target == current_hart {
            if let Err(error) = execute_local(request)
                && result.is_ok()
            {
                result = fence_error(request.operation(), error);
            }
            continue;
        }

        let remote = remote_rfence_for(target);
        local.add();
        remote.set(request);

        if ipi::get().is_some_and(|ipi| ipi.send(IpiEvent::Rfence, [target]).is_ok()) {
            continue;
        }
        // A receiver that already took the request remains responsible for
        // its acknowledgement.
        if remote.cancel(current_hart) {
            remote_rfence_for(current_hart).sub();
        }
        result = SbiRet::failed();
        break;
    }

    // Complete previously submitted operations even if a later send failed.
    while !local.is_sync() {
        rfence_poll_for(current_hart);
    }

    if let Some(error) = local.take_error()
        && result.is_ok()
    {
        result = fence_error(request.operation(), error);
    }

    result
}

/// Validates a request before it becomes visible to remote harts.
fn remote_fence(
    operation: FenceOperation,
    start_addr: usize,
    size: usize,
    hart_mask: HartMask,
    sent_event: usize,
) -> SbiRet {
    // HFENCE requires an H-capable caller even when there are no targets.
    #[cfg(feature = "hypervisor")]
    if operation.requires_hypervisor() && !runtime::hart::has_hypervisor() {
        return SbiRet::not_supported();
    }
    pmu_firmware_counter_increment(sent_event);
    let request =
        match LocalFence::current().and_then(|local| local.request(operation, start_addr, size)) {
            Ok(request) => request,
            Err(error) => return fence_error(operation, error),
        };
    remote_fence_process(request, hart_mask)
}

fn fence_error(operation: FenceOperation, error: Error) -> SbiRet {
    match error {
        Error::InvalidArgs | Error::Overflow if operation != FenceOperation::FenceI => {
            SbiRet::invalid_address()
        }
        Error::NotSupported | Error::IllegalInstruction if operation.requires_hypervisor() => {
            SbiRet::not_supported()
        }
        _ => SbiRet::failed(),
    }
}

impl Fence for SbiRFence {
    /// Executes an instruction fence on the specified harts.
    fn remote_fence_i(&self, hart_mask: HartMask) -> SbiRet {
        remote_fence(
            FenceOperation::FenceI,
            0,
            0,
            hart_mask,
            firmware_event::FENCE_I_SENT,
        )
    }

    /// Executes a supervisor virtual-memory fence on the specified harts.
    fn remote_sfence_vma(&self, hart_mask: HartMask, start_addr: usize, size: usize) -> SbiRet {
        remote_fence(
            FenceOperation::SFenceVma,
            start_addr,
            size,
            hart_mask,
            firmware_event::SFENCE_VMA_SENT,
        )
    }

    /// Executes a supervisor virtual-memory fence for an ASID on the specified harts.
    fn remote_sfence_vma_asid(
        &self,
        hart_mask: HartMask,
        start_addr: usize,
        size: usize,
        asid: usize,
    ) -> SbiRet {
        remote_fence(
            FenceOperation::SFenceVmaAsid { asid },
            start_addr,
            size,
            hart_mask,
            firmware_event::SFENCE_VMA_ASID_SENT,
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
        remote_fence(
            FenceOperation::HFenceGvmaVmid { vmid },
            start_addr,
            size,
            hart_mask,
            firmware_event::HFENCE_GVMA_VMID_SENT,
        )
    }

    #[cfg(feature = "hypervisor")]
    fn remote_hfence_gvma(&self, hart_mask: HartMask, start_addr: usize, size: usize) -> SbiRet {
        remote_fence(
            FenceOperation::HFenceGvma,
            start_addr,
            size,
            hart_mask,
            firmware_event::HFENCE_GVMA_SENT,
        )
    }

    #[cfg(feature = "hypervisor")]
    fn remote_hfence_vvma_asid(
        &self,
        hart_mask: HartMask,
        start_addr: usize,
        size: usize,
        asid: usize,
    ) -> SbiRet {
        remote_fence(
            FenceOperation::HFenceVvmaAsid { asid },
            start_addr,
            size,
            hart_mask,
            firmware_event::HFENCE_VVMA_ASID_SENT,
        )
    }

    #[cfg(feature = "hypervisor")]
    fn remote_hfence_vvma(&self, hart_mask: HartMask, start_addr: usize, size: usize) -> SbiRet {
        remote_fence(
            FenceOperation::HFenceVvma,
            start_addr,
            size,
            hart_mask,
            firmware_event::HFENCE_VVMA_SENT,
        )
    }
}

/// Services one queued request while waiting outside the IPI handler.
#[inline]
pub(crate) fn rfence_poll_for(hart: HartId) {
    rfence_single_handler_for(hart);
}

/// Handles one fence operation queued for `hart`, returning whether one was dequeued.
#[inline]
fn rfence_single_handler_for(hart: HartId) -> bool {
    let Some((request, source_hart)) = local_rfence_for(hart).get() else {
        return false;
    };
    // A dequeued remote request counts as received even if execution fails.
    record_received_fence(request.operation());
    let result = execute_local(request);
    remote_rfence_for(source_hart).complete(result);
    true
}

#[inline]
fn execute_local(request: FenceRequest) -> Result<()> {
    LocalFence::current()?.execute(request, TLB_FLUSH_LIMIT)
}

fn record_received_fence(operation: FenceOperation) {
    let event = match operation {
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
    pmu_firmware_counter_increment(event);
}

/// Processes all pending remote fence operations on the current hart.
#[inline]
fn rfence_handler(hart: HartId) {
    while rfence_single_handler_for(hart) {}
}
