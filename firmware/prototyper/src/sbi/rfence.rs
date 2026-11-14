//! Remote fence operations.
//!
//! # References
//!
//! - Specification: [RISC-V SBI RFENCE extension](https://docs.riscv.org/reference/sbi/v3.0/ext-rfence.html) —
//!   hart-mask and address-range semantics for remote fences.
//! - Specification: [RISC-V hypervisor extension](https://docs.riscv.org/reference/isa/priv/hypervisor.html) —
//!   hypervisor fence instructions and their execution requirements.

use runtime::{
    Error, Result,
    hart::HartId,
    ipi::{self, IpiEvent},
    rustsbi::{Fence, HartMask, SbiRet},
};
use sbi_spec::pmu::firmware_event;

use crate::cfg::{PAGE_SIZE, TLB_FLUSH_LIMIT};
use core::arch::asm;

#[cfg(feature = "hypervisor")]
mod policy;
pub(super) mod queue;
use super::{ipi::target_harts, pmu::pmu_firmware_counter_increment};
use queue::{LocalRFenceCell, RemoteRFenceCell};

/// Context information for a remote fence operation.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct RFenceContext {
    /// Start address of memory region to fence.
    pub start_addr: usize,
    /// Size of memory region to fence.
    pub size: usize,
    /// Address space ID.
    pub asid: usize,
    /// Virtual machine ID.
    pub vmid: usize,
    /// Type of fence operation.
    pub op: RFenceType,
}

/// Types of remote fence operations supported.
#[allow(unused)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RFenceType {
    /// Instruction fence.
    FenceI,
    /// Supervisor fence for virtual memory.
    SFenceVma,
    /// Supervisor fence for virtual memory with ASID.
    SFenceVmaAsid,
    #[cfg(feature = "hypervisor")]
    /// Hypervisor fence for guest physical memory with a VMID.
    HFenceGvmaVmid,
    #[cfg(feature = "hypervisor")]
    /// Hypervisor fence for guest physical memory.
    HFenceGvma,
    #[cfg(feature = "hypervisor")]
    /// Hypervisor fence for guest virtual memory with an ASID.
    HFenceVvmaAsid,
    #[cfg(feature = "hypervisor")]
    /// Hypervisor fence for guest virtual memory.
    HFenceVvma,
}

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

/// Returns a remote view of the fence state for an enabled hart.
pub(crate) fn remote_rfence(hart_id: usize) -> Option<RemoteRFenceCell<'static>> {
    Some(remote_rfence_for(HartId::from_raw(hart_id).ok()?))
}

#[allow(unused)]
impl LocalRFenceCell<'_> {
    /// Adds a fence operation to the queue, retrying while it is full or busy.
    pub fn set(&self, ctx: RFenceContext) {
        let hart = HartId::current().expect("BUG: current hart exceeds Runtime capacity");
        loop {
            if self.try_push((ctx, hart.as_usize())) {
                break;
            }
            rfence_poll_for(hart);
        }
    }
}

#[allow(unused)]
impl RemoteRFenceCell<'_> {
    /// Adds a fence operation to the queue from a remote hart.
    ///
    /// Retries while the queue is full or busy.
    pub fn set(&self, ctx: RFenceContext) {
        let hart = HartId::current().expect("BUG: current hart exceeds Runtime capacity");
        loop {
            if self.try_push((ctx, hart.as_usize())) {
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

/// Validates the address range for fence operations.
#[inline(always)]
fn validate_address_range(start_addr: usize, size: usize) -> Result<usize, SbiRet> {
    if !((start_addr == 0 && size == 0) || size == usize::MAX) && start_addr & (PAGE_SIZE - 1) != 0
    {
        return Err(SbiRet::invalid_address());
    }

    if start_addr > usize::MAX - size {
        return Err(SbiRet::invalid_address());
    }

    Ok(size)
}

/// Processes a remote fence operation by sending IPI to target harts.
fn remote_fence_process(rfence_ctx: RFenceContext, hart_mask: HartMask) -> SbiRet {
    let targets = match target_harts(hart_mask) {
        Ok(targets) => targets,
        Err(error) => return error,
    };

    #[cfg(feature = "hypervisor")]
    {
        if matches!(
            rfence_ctx.op,
            RFenceType::HFenceGvmaVmid
                | RFenceType::HFenceGvma
                | RFenceType::HFenceVvmaAsid
                | RFenceType::HFenceVvma
        ) {
            // HFENCE legality depends on every target, so the whole call must
            // be rejected before any target — including the local one —
            // executes, enqueues, or receives an IPI. The snapshot also fixes
            // the dispatched set to the one that was validated.
            let Some(targets) = policy::preflight_hfence_targets(targets, |hart| {
                super::features::has_extension(*hart, super::features::Extension::Hypervisor)
            }) else {
                return SbiRet::not_supported();
            };
            return remote_fence_dispatch(rfence_ctx, targets);
        }
    }

    remote_fence_dispatch(rfence_ctx, targets)
}

/// Dispatches a fence to the pre-validated targets and waits for completion.
fn remote_fence_dispatch(
    rfence_ctx: RFenceContext,
    targets: impl IntoIterator<Item = HartId>,
) -> SbiRet {
    let current_hart = HartId::current().expect("BUG: current hart is not in the boot topology");
    let local = local_rfence_for(current_hart);
    let mut result = SbiRet::success(0);

    for target in targets {
        if target == current_hart {
            rfence_local_handler(rfence_ctx);
            continue;
        }

        let remote = remote_rfence_for(target);
        local.add();
        remote.set(rfence_ctx);

        if ipi::get().is_some_and(|ipi| ipi.send(IpiEvent::Rfence, [target]).is_ok()) {
            continue;
        }
        // A receiver that already took the request remains responsible for
        // its acknowledgement.
        if remote.cancel(current_hart.as_usize()) {
            remote_rfence_for(current_hart).sub();
        }
        result = SbiRet::failed();
        break;
    }

    // Complete previously submitted operations even if a later send failed.
    while !local.is_sync() {
        rfence_poll_for(current_hart);
    }

    result
}

#[cfg(feature = "hypervisor")]
fn supports_hypervisor_extension() -> bool {
    let hart = HartId::current().expect("BUG: current hart exceeds Runtime capacity");
    super::features::has_extension(hart, super::features::Extension::Hypervisor)
}

impl Fence for SbiRFence {
    /// Executes an instruction fence on the specified harts.
    fn remote_fence_i(&self, hart_mask: HartMask) -> SbiRet {
        pmu_firmware_counter_increment(firmware_event::FENCE_I_SENT);
        remote_fence_process(
            RFenceContext {
                start_addr: 0,
                size: 0,
                asid: 0,
                vmid: 0,
                op: RFenceType::FenceI,
            },
            hart_mask,
        )
    }

    /// Executes a supervisor virtual-memory fence on the specified harts.
    fn remote_sfence_vma(&self, hart_mask: HartMask, start_addr: usize, size: usize) -> SbiRet {
        pmu_firmware_counter_increment(firmware_event::SFENCE_VMA_SENT);
        let flush_size = match validate_address_range(start_addr, size) {
            Ok(size) => size,
            Err(e) => return e,
        };

        remote_fence_process(
            RFenceContext {
                start_addr,
                size: flush_size,
                asid: 0,
                vmid: 0,
                op: RFenceType::SFenceVma,
            },
            hart_mask,
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
        pmu_firmware_counter_increment(firmware_event::SFENCE_VMA_ASID_SENT);
        let flush_size = match validate_address_range(start_addr, size) {
            Ok(size) => size,
            Err(e) => return e,
        };

        remote_fence_process(
            RFenceContext {
                start_addr,
                size: flush_size,
                asid,
                vmid: 0,
                op: RFenceType::SFenceVmaAsid,
            },
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
        if !supports_hypervisor_extension() {
            return SbiRet::not_supported();
        }
        pmu_firmware_counter_increment(firmware_event::HFENCE_GVMA_VMID_SENT);

        let flush_size = match validate_address_range(start_addr, size) {
            Ok(s) => s,
            Err(e) => return e,
        };

        remote_fence_process(
            RFenceContext {
                start_addr,
                size: flush_size,
                asid: 0,
                vmid,
                op: RFenceType::HFenceGvmaVmid,
            },
            hart_mask,
        )
    }

    #[cfg(feature = "hypervisor")]
    fn remote_hfence_gvma(&self, hart_mask: HartMask, start_addr: usize, size: usize) -> SbiRet {
        if !supports_hypervisor_extension() {
            return SbiRet::not_supported();
        }
        pmu_firmware_counter_increment(firmware_event::HFENCE_GVMA_SENT);

        let flush_size = match validate_address_range(start_addr, size) {
            Ok(s) => s,
            Err(e) => return e,
        };

        remote_fence_process(
            RFenceContext {
                start_addr,
                size: flush_size,
                asid: 0,
                vmid: 0,
                op: RFenceType::HFenceGvma,
            },
            hart_mask,
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
        if !supports_hypervisor_extension() {
            return SbiRet::not_supported();
        }
        pmu_firmware_counter_increment(firmware_event::HFENCE_VVMA_ASID_SENT);

        let flush_size = match validate_address_range(start_addr, size) {
            Ok(s) => s,
            Err(e) => return e,
        };

        remote_fence_process(
            RFenceContext {
                start_addr,
                size: flush_size,
                asid,
                vmid: 0,
                op: RFenceType::HFenceVvmaAsid,
            },
            hart_mask,
        )
    }

    #[cfg(feature = "hypervisor")]
    fn remote_hfence_vvma(&self, hart_mask: HartMask, start_addr: usize, size: usize) -> SbiRet {
        if !supports_hypervisor_extension() {
            return SbiRet::not_supported();
        }
        pmu_firmware_counter_increment(firmware_event::HFENCE_VVMA_SENT);

        let flush_size = match validate_address_range(start_addr, size) {
            Ok(s) => s,
            Err(e) => return e,
        };

        remote_fence_process(
            RFenceContext {
                start_addr,
                size: flush_size,
                asid: 0,
                vmid: 0,
                op: RFenceType::HFenceVvma,
            },
            hart_mask,
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
    let local_rf = local_rfence_for(hart);

    if let Some((ctx, source_hart_id)) = local_rf.get() {
        rfence_local_handler(ctx);
        remote_rfence(source_hart_id).unwrap().sub();
        true
    } else {
        false
    }
}

/// Executes a fence on this hart and records its received PMU event.
#[inline]
pub(crate) fn rfence_local_handler(ctx: RFenceContext) {
    let full_flush = (ctx.start_addr == 0 && ctx.size == 0)
        || (ctx.size == usize::MAX)
        || (ctx.size > TLB_FLUSH_LIMIT && ctx.size != usize::MAX);

    // Every HFENCE queue entry passed the target hypervisor-extension
    // preflight before dispatch; the fences only synchronize
    // address-translation state.
    match ctx.op {
        RFenceType::FenceI => {
            pmu_firmware_counter_increment(firmware_event::FENCE_I_RECEIVED);
            runtime::instruction::fence_i();
        }
        RFenceType::SFenceVma => {
            pmu_firmware_counter_increment(firmware_event::SFENCE_VMA_RECEIVED);
            if full_flush {
                runtime::instruction::sfence_vma_all();
            } else {
                for offset in (0..ctx.size).step_by(PAGE_SIZE) {
                    let addr = ctx.start_addr.wrapping_add(offset);
                    // SAFETY: M-mode executes the requested address-scoped fence.
                    unsafe { asm!("sfence.vma {}, x0", in(reg) addr, options(nostack)) };
                }
            }
        }
        RFenceType::SFenceVmaAsid => {
            pmu_firmware_counter_increment(firmware_event::SFENCE_VMA_ASID_RECEIVED);
            let asid = ctx.asid;
            if full_flush {
                // SAFETY: M-mode executes the requested ASID-scoped fence.
                unsafe { asm!("sfence.vma x0, {}", in(reg) asid, options(nostack)) };
            } else {
                for offset in (0..ctx.size).step_by(PAGE_SIZE) {
                    let addr = ctx.start_addr.wrapping_add(offset);
                    runtime::instruction::sfence_vma(asid, addr);
                }
            }
        }
        #[cfg(feature = "hypervisor")]
        RFenceType::HFenceGvmaVmid => {
            pmu_firmware_counter_increment(firmware_event::HFENCE_GVMA_VMID_RECEIVED);
            let vmid = ctx.vmid;
            if full_flush {
                // SAFETY: Every HFENCE queue entry passed the target
                // hypervisor preflight against the firmware's feature
                // records; the fence only synchronizes translations.
                unsafe { asm!("hfence.gvma x0, {}", in(reg) vmid, options(nostack)) };
            } else {
                for offset in (0..ctx.size).step_by(PAGE_SIZE) {
                    let addr = ctx.start_addr.wrapping_add(offset);
                    // The `rs1` operand takes the guest physical page number:
                    // the byte GPA is shifted right by two, as required by
                    // `hfence.gvma`.
                    // SAFETY: Every HFENCE queue entry passed the target
                    // hypervisor preflight against the firmware's feature
                    // records; the fence only synchronizes translations.
                    unsafe {
                        asm!(
                            "hfence.gvma {}, {}",
                            in(reg) policy::encode_gvma_address(addr),
                            in(reg) vmid,
                            options(nostack)
                        )
                    };
                }
            }
        }
        #[cfg(feature = "hypervisor")]
        RFenceType::HFenceGvma => {
            pmu_firmware_counter_increment(firmware_event::HFENCE_GVMA_RECEIVED);
            if full_flush {
                // SAFETY: Every HFENCE queue entry passed the target
                // hypervisor preflight against the firmware's feature
                // records; the fence only synchronizes translations.
                unsafe { asm!("hfence.gvma x0, x0", options(nostack)) };
            } else {
                for offset in (0..ctx.size).step_by(PAGE_SIZE) {
                    let addr = ctx.start_addr.wrapping_add(offset);
                    // The `rs1` operand takes the guest physical page number:
                    // the byte GPA is shifted right by two, as required by
                    // `hfence.gvma`.
                    // SAFETY: Every HFENCE queue entry passed the target
                    // hypervisor preflight against the firmware's feature
                    // records; the fence only synchronizes translations.
                    unsafe {
                        asm!("hfence.gvma {}, x0", in(reg) policy::encode_gvma_address(addr), options(nostack))
                    };
                }
            }
        }
        #[cfg(feature = "hypervisor")]
        RFenceType::HFenceVvmaAsid => {
            pmu_firmware_counter_increment(firmware_event::HFENCE_VVMA_ASID_RECEIVED);
            let asid = ctx.asid;
            if full_flush {
                // SAFETY: Every HFENCE queue entry passed the target
                // hypervisor preflight against the firmware's feature
                // records; the fence only synchronizes translations.
                unsafe { asm!("hfence.vvma x0, {}", in(reg) asid, options(nostack)) };
            } else {
                for offset in (0..ctx.size).step_by(PAGE_SIZE) {
                    let addr = ctx.start_addr.wrapping_add(offset);
                    // SAFETY: Every HFENCE queue entry passed the target
                    // hypervisor preflight against the firmware's feature
                    // records; the fence only synchronizes translations.
                    unsafe {
                        asm!("hfence.vvma {}, {}", in(reg) addr, in(reg) asid, options(nostack))
                    };
                }
            }
        }
        #[cfg(feature = "hypervisor")]
        RFenceType::HFenceVvma => {
            pmu_firmware_counter_increment(firmware_event::HFENCE_VVMA_RECEIVED);
            if full_flush {
                // SAFETY: Every HFENCE queue entry passed the target
                // hypervisor preflight against the firmware's feature
                // records; the fence only synchronizes translations.
                unsafe { asm!("hfence.vvma x0, x0", options(nostack)) };
            } else {
                for offset in (0..ctx.size).step_by(PAGE_SIZE) {
                    let addr = ctx.start_addr.wrapping_add(offset);
                    // SAFETY: Every HFENCE queue entry passed the target
                    // hypervisor preflight against the firmware's feature
                    // records; the fence only synchronizes translations.
                    unsafe { asm!("hfence.vvma {}, x0", in(reg) addr, options(nostack)) };
                }
            }
        }
    }
}

/// Processes all pending remote fence operations on the current hart.
#[inline]
fn rfence_handler(hart: HartId) {
    while rfence_single_handler_for(hart) {}
}
