//! SBI RFence (Remote Fence) extension.

use rustsbi::{HartMask, SbiRet};
use sbi_spec::pmu::firmware_event;

use crate::cfg::{PAGE_SIZE, TLB_FLUSH_LIMIT};
use crate::riscv::current_hartid;
use core::arch::asm;

use super::pmu::pmu_firmware_counter_increment;
use super::trap_stack::{FifoError, LocalRFenceCell, RemoteRFenceCell};

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
            match self.try_push((ctx, hart_id)) {
                Ok(_) => break,
                Err(FifoError::Full) => rfence_single_handler(),
                Err(_) => panic!("Unable to push fence ops to fifo"),
            }
        }
    }
}

#[allow(unused)]
impl RemoteRFenceCell<'_> {
    /// Adds a fence operation to the queue from a remote hart.
    pub fn set(&self, ctx: RFenceContext) {
        let hart_id = current_hartid();
        loop {
            match self.try_push((ctx, hart_id)) {
                Ok(_) => return,
                Err(FifoError::Full) => rfence_single_handler(),
                Err(_) => panic!("Unable to push fence ops to fifo"),
            }
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
    super::features::hart_extension_probe(current_hartid(), super::features::Extension::Hypervisor)
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

/// Handles a single remote fence operation.
#[inline]
pub fn rfence_single_handler() {
    let local_rf = match local_rfence() {
        Some(lr) => lr,
        // TODO: Or return an error, depending on expected invariants
        None => panic!("rfence_single_handler called with no local rfence context"),
    };

    if let Some((ctx, source_hart_id)) = local_rf.get() {
        let full_flush = (ctx.start_addr == 0 && ctx.size == 0)
            || (ctx.size == usize::MAX)
            || (ctx.size > TLB_FLUSH_LIMIT && ctx.size != usize::MAX);

        match ctx.op {
            RFenceType::FenceI => {
                pmu_firmware_counter_increment(firmware_event::FENCE_I_RECEIVED);
                unsafe { asm!("fence.i") };
                remote_rfence(source_hart_id).unwrap().sub();
            }
            RFenceType::SFenceVma => {
                pmu_firmware_counter_increment(firmware_event::SFENCE_VMA_RECEIVED);
                if full_flush {
                    unsafe { asm!("sfence.vma") };
                } else {
                    for offset in (0..ctx.size).step_by(PAGE_SIZE) {
                        let addr = ctx.start_addr.wrapping_add(offset);
                        unsafe { asm!("sfence.vma {}", in(reg) addr) };
                    }
                }
                if let Some(remote_cell) = remote_rfence(source_hart_id) {
                    remote_cell.sub();
                }
            }
            RFenceType::SFenceVmaAsid => {
                pmu_firmware_counter_increment(firmware_event::SFENCE_VMA_ASID_RECEIVED);
                let asid = ctx.asid;
                if full_flush {
                    unsafe { asm!("sfence.vma x0, {}", in(reg) asid) };
                } else {
                    for offset in (0..ctx.size).step_by(PAGE_SIZE) {
                        let addr = ctx.start_addr.wrapping_add(offset);
                        unsafe { asm!("sfence.vma {}, {}", in(reg) addr, in(reg) asid) };
                    }
                }
                if let Some(remote_cell) = remote_rfence(source_hart_id) {
                    remote_cell.sub();
                }
            }
            #[cfg(feature = "hypervisor")]
            RFenceType::HFenceGvmaVmid => {
                pmu_firmware_counter_increment(firmware_event::HFENCE_GVMA_VMID_RECEIVED);
                let vmid = ctx.vmid;
                if full_flush {
                    unsafe { asm!("hfence.gvma x0, {}", in(reg) vmid) };
                } else {
                    for offset in (0..ctx.size).step_by(PAGE_SIZE) {
                        let addr = ctx.start_addr.wrapping_add(offset);
                        unsafe { asm!("hfence.gvma {}, {}", in(reg) addr, in(reg) vmid) };
                    }
                }
                if let Some(remote_cell) = remote_rfence(source_hart_id) {
                    remote_cell.sub();
                }
            }
            #[cfg(feature = "hypervisor")]
            RFenceType::HFenceGvma => {
                pmu_firmware_counter_increment(firmware_event::HFENCE_GVMA_RECEIVED);
                if full_flush {
                    unsafe { asm!("hfence.gvma x0, x0") };
                } else {
                    for offset in (0..ctx.size).step_by(PAGE_SIZE) {
                        let addr = ctx.start_addr.wrapping_add(offset);
                        unsafe { asm!("hfence.gvma {}, x0", in(reg) addr) };
                    }
                }
                if let Some(remote_cell) = remote_rfence(source_hart_id) {
                    remote_cell.sub();
                }
            }
            #[cfg(feature = "hypervisor")]
            RFenceType::HFenceVvmaAsid => {
                pmu_firmware_counter_increment(firmware_event::HFENCE_VVMA_ASID_RECEIVED);
                let asid = ctx.asid;
                if full_flush {
                    unsafe { asm!("hfence.vvma x0, {}", in(reg) asid) };
                } else {
                    for offset in (0..ctx.size).step_by(PAGE_SIZE) {
                        let addr = ctx.start_addr.wrapping_add(offset);
                        unsafe { asm!("hfence.vvma {}, {}", in(reg) addr, in(reg) asid) };
                    }
                }
                if let Some(remote_cell) = remote_rfence(source_hart_id) {
                    remote_cell.sub();
                }
            }
            #[cfg(feature = "hypervisor")]
            RFenceType::HFenceVvma => {
                pmu_firmware_counter_increment(firmware_event::HFENCE_VVMA_RECEIVED);
                if full_flush {
                    unsafe { asm!("hfence.vvma x0, x0") };
                } else {
                    for offset in (0..ctx.size).step_by(PAGE_SIZE) {
                        let addr = ctx.start_addr.wrapping_add(offset);
                        unsafe { asm!("hfence.vvma {}, x0", in(reg) addr) };
                    }
                }
                if let Some(remote_cell) = remote_rfence(source_hart_id) {
                    remote_cell.sub();
                }
            }
        }
    }
}

/// Process all pending remote fence operations on the current hart.
#[inline]
pub fn rfence_handler() {
    while !local_rfence().unwrap().is_empty() {
        rfence_single_handler();
    }
}
