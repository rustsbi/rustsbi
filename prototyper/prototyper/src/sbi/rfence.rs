use rustsbi::{HartMask, SbiRet};
use sbi_spec::pmu::firmware_event;
use spin::Mutex;

use crate::cfg::{PAGE_SIZE, TLB_FLUSH_LIMIT};
use crate::platform::PLATFORM;
use crate::riscv::current_hartid;
use crate::sbi::fifo::{Fifo, FifoError};
use crate::sbi::trap_stack::ROOT_STACK;
use core::arch::asm;

use core::sync::atomic::{AtomicU32, Ordering};

use super::pmu::pmu_firmware_counter_increment;

/// Cell for managing remote fence operations between harts.
pub(crate) struct RFenceCell {
    // Queue of fence operations with source hart ID
    queue: Mutex<Fifo<(RFenceContext, usize)>>,
    // Counter for tracking pending synchronization operations
    wait_sync_count: AtomicU32,
}

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
#[derive(Clone, Copy, Debug)]
pub enum RFenceType {
    /// Instruction fence.
    FenceI,
    /// Supervisor fence for virtual memory.
    SFenceVma,
    /// Supervisor fence for virtual memory with ASID.
    SFenceVmaAsid,
    /// Hypervisor fence for guest virtual memory with VMID.
    HFenceGvmaVmid,
    /// Hypervisor fence for guest virtual memory.
    HFenceGvma,
    /// Hypervisor fence for virtual machine virtual memory with ASID.
    HFenceVvmaAsid,
    /// Hypervisor fence for virtual machine virtual memory.
    HFenceVvma,
}

impl RFenceCell {
    /// Creates a new RFenceCell with empty queue and zero sync count.
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(Fifo::new()),
            wait_sync_count: AtomicU32::new(0),
        }
    }

    /// Gets a local view of this fence cell for the current hart.
    #[inline]
    pub fn local(&self) -> LocalRFenceCell<'_> {
        LocalRFenceCell(self)
    }

    /// Gets a remote view of this fence cell for accessing from other harts.
    #[inline]
    pub fn remote(&self) -> RemoteRFenceCell<'_> {
        RemoteRFenceCell(self)
    }
}

// Mark RFenceCell as safe to share between threads
unsafe impl Sync for RFenceCell {}
unsafe impl Send for RFenceCell {}

/// View of RFenceCell for operations on the current hart.
pub struct LocalRFenceCell<'a>(&'a RFenceCell);

/// View of RFenceCell for operations from other harts.
pub struct RemoteRFenceCell<'a>(&'a RFenceCell);

/// Gets the local fence context for the current hart.
pub(crate) fn local_rfence() -> Option<LocalRFenceCell<'static>> {
    unsafe {
        ROOT_STACK
            .get_mut(current_hartid())
            .map(|x| x.hart_context().rfence.local())
    }
}

/// Gets the remote fence context for a specific hart.
pub(crate) fn remote_rfence(hart_id: usize) -> Option<RemoteRFenceCell<'static>> {
    unsafe {
        ROOT_STACK
            .get_mut(hart_id)
            .map(|x| x.hart_context().rfence.remote())
    }
}

#[allow(unused)]
impl LocalRFenceCell<'_> {
    /// Checks if all synchronization operations are complete.
    pub fn is_sync(&self) -> bool {
        self.0.wait_sync_count.load(Ordering::Relaxed) == 0
    }

    /// Increments the synchronization counter.
    pub fn add(&self) {
        self.0.wait_sync_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Checks if the operation queue is empty.
    pub fn is_empty(&self) -> bool {
        self.0.queue.lock().is_empty()
    }

    /// Gets the next fence operation from the queue.
    pub fn get(&self) -> Option<(RFenceContext, usize)> {
        self.0.queue.lock().pop().ok()
    }

    /// Adds a fence operation to the queue, retrying if full.
    pub fn set(&self, ctx: RFenceContext) {
        let hart_id = current_hartid();
        loop {
            let mut queue = self.0.queue.lock();
            match queue.push((ctx, hart_id)) {
                Ok(_) => break,
                Err(FifoError::Full) => {
                    drop(queue);
                    rfence_single_handler();
                }
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
            let mut queue = self.0.queue.lock();
            match queue.push((ctx, hart_id)) {
                Ok(_) => return,
                Err(FifoError::Full) => {
                    drop(queue);
                    rfence_single_handler();
                }
                Err(_) => panic!("Unable to push fence ops to fifo"),
            }
        }
    }

    /// Decrements the synchronization counter.
    pub fn sub(&self) {
        self.0.wait_sync_count.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Implementation of RISC-V remote fence operations.
pub(crate) struct SbiRFence;

/// Validates address range for fence operations
#[inline(always)]
fn validate_address_range(start_addr: usize, size: usize) -> Result<usize, SbiRet> {
    // Check page alignment using bitwise AND instead of modulo
    if start_addr & 0xFFF != 0 {
        return Err(SbiRet::invalid_address());
    }

    // Avoid checked_add by checking for overflow directly
    if size > usize::MAX - start_addr {
        return Err(SbiRet::invalid_address());
    }

    Ok(size)
}

/// Processes a remote fence operation by sending IPI to target harts.
fn remote_fence_process(rfence_ctx: RFenceContext, hart_mask: HartMask) -> SbiRet {
    let sbi_ret = unsafe { PLATFORM.sbi.ipi.as_ref() }
        .unwrap()
        .send_ipi_by_fence(hart_mask, rfence_ctx);

    sbi_ret
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
}

/// Handles a single remote fence operation.
#[inline]
pub fn rfence_single_handler() {
    let rfence_context = local_rfence().unwrap().get();
    if let Some((ctx, id)) = rfence_context {
        match ctx.op {
            // Handle instruction fence
            RFenceType::FenceI => unsafe {
                pmu_firmware_counter_increment(firmware_event::FENCE_I_RECEIVED);
                asm!("fence.i");
                remote_rfence(id).unwrap().sub();
            },
            // Handle virtual memory address fence
            RFenceType::SFenceVma => {
                pmu_firmware_counter_increment(firmware_event::SFENCE_VMA_RECEIVED);
                // If the flush size is greater than the maximum limit then simply flush all
                if (ctx.start_addr == 0 && ctx.size == 0)
                    || (ctx.size == usize::MAX)
                    || (ctx.size > TLB_FLUSH_LIMIT)
                {
                    unsafe {
                        asm!("sfence.vma");
                    }
                } else {
                    for offset in (0..ctx.size).step_by(PAGE_SIZE) {
                        let addr = ctx.start_addr + offset;
                        unsafe {
                            asm!("sfence.vma {}", in(reg) addr);
                        }
                    }
                }
                remote_rfence(id).unwrap().sub();
            }
            // Handle virtual memory address fence with ASID
            RFenceType::SFenceVmaAsid => {
                pmu_firmware_counter_increment(firmware_event::SFENCE_VMA_ASID_RECEIVED);
                let asid = ctx.asid;
                // If the flush size is greater than the maximum limit then simply flush all
                if (ctx.start_addr == 0 && ctx.size == 0)
                    || (ctx.size == usize::MAX)
                    || (ctx.size > TLB_FLUSH_LIMIT)
                {
                    unsafe {
                        asm!("sfence.vma x0, {}", in(reg) asid);
                    }
                } else {
                    for offset in (0..ctx.size).step_by(PAGE_SIZE) {
                        let addr = ctx.start_addr + offset;
                        unsafe {
                            asm!("sfence.vma {}, {}", in(reg) addr, in(reg) asid);
                        }
                    }
                }
                remote_rfence(id).unwrap().sub();
            }
            rfencetype => {
                error!("Unsupported RFence Type: {:?}!", rfencetype);
            }
        }
    }
}

/// Process all pending remote fence operations.
#[inline]
pub fn rfence_handler() {
    while !local_rfence().unwrap().is_empty() {
        rfence_single_handler();
    }
}
