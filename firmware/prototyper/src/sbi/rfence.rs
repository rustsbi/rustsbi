//! Remote fence operations.
//!
//! # References
//!
//! - Specification: [RISC-V SBI RFENCE extension](https://docs.riscv.org/reference/sbi/v3.0/ext-rfence.html) —
//!   hart-mask and address-range semantics for remote fences.

#![forbid(unsafe_code)]

use rustsbi::{HartMask, SbiRet};
use sbi_spec::pmu::firmware_event;

use crate::cfg::{PAGE_SIZE, TLB_FLUSH_LIMIT};
use crate::riscv::csr::fence;
use crate::riscv::current_hartid;

use super::pmu::pmu_firmware_counter_increment;
use super::trap_stack::{LocalRFenceCell, RemoteRFenceCell};

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
    /// Hypervisor fence for guest virtual memory with VMID.
    HFenceGvmaVmid,
    #[cfg(feature = "hypervisor")]
    /// Hypervisor fence for guest virtual memory.
    HFenceGvma,
    #[cfg(feature = "hypervisor")]
    /// Hypervisor fence for guest virtual memory with ASID.
    HFenceVvmaAsid,
    #[cfg(feature = "hypervisor")]
    /// Hypervisor fence for guest virtual memory.
    HFenceVvma,
}

/// Gets the local fence context for the current hart.
pub(crate) use super::trap_stack::local_rfence;
/// Gets the remote fence context for a specific hart.
pub(crate) use super::trap_stack::remote_rfence;

#[allow(unused)]
impl LocalRFenceCell<'_> {
    /// Adds a fence operation to the queue, retrying if full.
    pub fn set(&self, ctx: RFenceContext) {
        let hart_id = current_hartid();
        loop {
            if self.try_push((ctx, hart_id)) {
                break;
            }
            rfence_poll();
        }
    }
}

#[allow(unused)]
impl RemoteRFenceCell<'_> {
    /// Adds a fence operation to the queue from a remote hart.
    pub fn set(&self, ctx: RFenceContext) {
        let hart_id = current_hartid();
        loop {
            if self.try_push((ctx, hart_id)) {
                return;
            }
            rfence_poll();
        }
    }
}

/// Implementation of RISC-V remote fence operations.
pub(crate) struct SbiRFence;

/// Validates address range for fence operations
#[inline(always)]
fn validate_address_range(start_addr: usize, size: usize) -> Result<usize, SbiRet> {
    if !((start_addr == 0 && size == 0) || size == usize::MAX) {
        if start_addr & (PAGE_SIZE - 1) != 0 {
            return Err(SbiRet::invalid_address());
        }
    }

    if start_addr > usize::MAX - size {
        return Err(SbiRet::invalid_address());
    }

    Ok(size)
}

/// Processes a remote fence operation by sending IPI to target harts.
fn remote_fence_process(rfence_ctx: RFenceContext, hart_mask: HartMask) -> SbiRet {
    let sbi_ret = crate::sbi::ipi()
        .unwrap()
        .send_ipi_by_fence(hart_mask, rfence_ctx);

    sbi_ret
}

#[cfg(feature = "hypervisor")]
fn supports_hypervisor_extension() -> bool {
    super::features::hart_has_extension(current_hartid(), super::features::Extension::Hypervisor)
}

impl rustsbi::Fence for SbiRFence {
    /// Remote instruction fence for specified harts.
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

    /// Remote supervisor fence for virtual memory on specified harts.
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

    /// Remote supervisor fence for virtual memory with ASID on specified harts.
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

/// Services requests while waiting outside the IPI handler.
///
/// Senders mark FENCE pending after enqueueing. Checking the existing bit
/// avoids locking an empty queue on each synchronization retry. A stale set
/// bit only causes an unnecessary dequeue attempt. The IPI handler clears
/// this bit before draining, so it must use the unconditional handler below.
#[inline]
pub(crate) fn rfence_poll() {
    use super::ipi::IPI_TYPE_FENCE;
    use super::trap_stack::hart_local;
    use core::sync::atomic::Ordering;

    let pending = hart_local(current_hartid())
        .ipi_type
        .load(Ordering::Relaxed);
    if pending & IPI_TYPE_FENCE != 0 {
        rfence_single_handler();
    }
}

/// Handles one remote fence operation, returning whether one was dequeued.
#[inline]
fn rfence_single_handler() -> bool {
    let local_rf = match local_rfence() {
        Some(lr) => lr,
        // TODO: Or return an error, depending on expected invariants
        None => panic!("rfence_single_handler called with no local rfence context"),
    };

    if let Some((ctx, source_hart_id)) = local_rf.get() {
        rfence_local_handler(ctx);
        remote_rfence(source_hart_id).unwrap().sub();
        true
    } else {
        false
    }
}

/// Executes a fence on this hart and records its received PMU event.
///
/// This function improves performance if RFence is runned on the local hart.
#[inline]
pub(crate) fn rfence_local_handler(ctx: RFenceContext) {
    let full_flush = (ctx.start_addr == 0 && ctx.size == 0)
        || (ctx.size == usize::MAX)
        || (ctx.size > TLB_FLUSH_LIMIT && ctx.size != usize::MAX);

    match ctx.op {
        RFenceType::FenceI => {
            pmu_firmware_counter_increment(firmware_event::FENCE_I_RECEIVED);
            fence::fence_i();
        }
        RFenceType::SFenceVma => {
            pmu_firmware_counter_increment(firmware_event::SFENCE_VMA_RECEIVED);
            if full_flush {
                fence::sfence_vma_all();
            } else {
                for offset in (0..ctx.size).step_by(PAGE_SIZE) {
                    let addr = ctx.start_addr.wrapping_add(offset);
                    fence::sfence_vma_addr(addr);
                }
            }
        }
        RFenceType::SFenceVmaAsid => {
            pmu_firmware_counter_increment(firmware_event::SFENCE_VMA_ASID_RECEIVED);
            let asid = ctx.asid;
            if full_flush {
                fence::sfence_vma_asid(asid);
            } else {
                for offset in (0..ctx.size).step_by(PAGE_SIZE) {
                    let addr = ctx.start_addr.wrapping_add(offset);
                    fence::sfence_vma_addr_asid(addr, asid);
                }
            }
        }
        #[cfg(feature = "hypervisor")]
        RFenceType::HFenceGvmaVmid => {
            pmu_firmware_counter_increment(firmware_event::HFENCE_GVMA_VMID_RECEIVED);
            let vmid = ctx.vmid;
            if full_flush {
                fence::hfence_gvma_vmid(vmid);
            } else {
                for offset in (0..ctx.size).step_by(PAGE_SIZE) {
                    let addr = ctx.start_addr.wrapping_add(offset);
                    fence::hfence_gvma_addr_vmid(addr, vmid);
                }
            }
        }
        #[cfg(feature = "hypervisor")]
        RFenceType::HFenceGvma => {
            pmu_firmware_counter_increment(firmware_event::HFENCE_GVMA_RECEIVED);
            if full_flush {
                fence::hfence_gvma_all();
            } else {
                for offset in (0..ctx.size).step_by(PAGE_SIZE) {
                    let addr = ctx.start_addr.wrapping_add(offset);
                    fence::hfence_gvma_addr(addr);
                }
            }
        }
        #[cfg(feature = "hypervisor")]
        RFenceType::HFenceVvmaAsid => {
            pmu_firmware_counter_increment(firmware_event::HFENCE_VVMA_ASID_RECEIVED);
            let asid = ctx.asid;
            if full_flush {
                fence::hfence_vvma_asid(asid);
            } else {
                for offset in (0..ctx.size).step_by(PAGE_SIZE) {
                    let addr = ctx.start_addr.wrapping_add(offset);
                    fence::hfence_vvma_addr_asid(addr, asid);
                }
            }
        }
        #[cfg(feature = "hypervisor")]
        RFenceType::HFenceVvma => {
            pmu_firmware_counter_increment(firmware_event::HFENCE_VVMA_RECEIVED);
            if full_flush {
                fence::hfence_vvma_all();
            } else {
                for offset in (0..ctx.size).step_by(PAGE_SIZE) {
                    let addr = ctx.start_addr.wrapping_add(offset);
                    fence::hfence_vvma_addr(addr);
                }
            }
        }
    }
}

/// Process all pending remote fence operations on the current hart.
#[inline]
pub fn rfence_handler() {
    while rfence_single_handler() {}
}
