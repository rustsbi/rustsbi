use rustsbi::{HartMask, SbiRet};
use spin::Mutex;

use crate::board::SBI;
use crate::fifo::{Fifo, FifoError};
use crate::riscv_spec::current_hartid;
use crate::trap_stack::NUM_HART_MAX;
use crate::trap_stack::ROOT_STACK;

use core::sync::atomic::{AtomicU32, Ordering};

pub(crate) struct RFenceCell {
    queue: Mutex<Fifo<(RFenceCTX, usize)>>, // (ctx, source_hart_id)
    wait_sync_count: AtomicU32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct RFenceCTX {
    pub start_addr: usize,
    pub size: usize,
    pub asid: usize,
    pub vmid: usize,
    pub op: RFenceType,
}

#[allow(unused)]
#[derive(Clone, Copy, Debug)]
pub(crate) enum RFenceType {
    FenceI,
    SFenceVma,
    SFenceVmaAsid,
    HFenceGvmaVmid,
    HFenceGvma,
    HFenceVvmaAsid,
    HFenceVvma,
}

impl RFenceCell {
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(Fifo::new()),
            wait_sync_count: AtomicU32::new(0),
        }
    }

    #[inline]
    pub unsafe fn local(&self) -> LocalRFenceCell<'_> {
        LocalRFenceCell(self)
    }

    /// 取出共享对象。
    #[inline]
    pub fn remote(&self) -> RemoteRFenceCell<'_> {
        RemoteRFenceCell(self)
    }
}

unsafe impl Sync for RFenceCell {}
unsafe impl Send for RFenceCell {}

/// 当前硬件线程的rfence上下文。
pub struct LocalRFenceCell<'a>(&'a RFenceCell);

/// 任意硬件线程的rfence上下文。
pub struct RemoteRFenceCell<'a>(&'a RFenceCell);

/// 获取当前hart的rfence上下文
pub(crate) fn local_rfence() -> LocalRFenceCell<'static> {
    unsafe {
        ROOT_STACK
            .get_unchecked_mut(current_hartid())
            .hart_context()
            .rfence
            .local()
    }
}

/// 获取任意hart的rfence上下文
pub(crate) fn remote_rfence(hart_id: usize) -> Option<RemoteRFenceCell<'static>> {
    unsafe {
        ROOT_STACK
            .get_mut(hart_id)
            .map(|x| x.hart_context().rfence.remote())
    }
}

#[allow(unused)]
impl LocalRFenceCell<'_> {
    pub fn is_zero(&self) -> bool {
        self.0.wait_sync_count.load(Ordering::Relaxed) == 0
    }
    pub fn add(&self) {
        self.0.wait_sync_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn is_empty(&self) -> bool {
        self.0.queue.lock().is_empty()
    }
    pub fn get(&self) -> Option<(RFenceCTX, usize)> {
        match self.0.queue.lock().pop() {
            Ok(res) => Some(res),
            Err(_) => None,
        }
    }
    pub fn set(&self, ctx: RFenceCTX) {
        loop {
            match self.0.queue.lock().push((ctx, current_hartid())) {
                Ok(_) => break (),
                Err(FifoError::NoChange) => {
                    crate::trap::rfence_signle_handler();
                    continue;
                }
                _ => panic!("Unable to push fence ops to fifo"),
            }
        }
    }
}

#[allow(unused)]
impl RemoteRFenceCell<'_> {
    pub fn set(&self, ctx: RFenceCTX) {
        // TODO: maybe deadlock
        loop {
            match self.0.queue.lock().push((ctx, current_hartid())) {
                Ok(_) => break (),
                Err(FifoError::NoChange) => {
                    crate::trap::rfence_signle_handler();
                    continue;
                }
                _ => panic!("Unable to push fence ops to fifo"),
            }
        }
    }

    pub fn sub(&self) {
        self.0.wait_sync_count.fetch_sub(1, Ordering::Relaxed);
    }
}

/// RFENCE
pub(crate) struct RFence;

fn remote_fence_process(rfence_ctx: RFenceCTX, hart_mask: HartMask) -> SbiRet {
    let sbi_ret = unsafe { SBI.assume_init_mut() }
        .clint
        .as_ref()
        .unwrap()
        .send_ipi_by_fence(hart_mask, rfence_ctx);

    if hart_mask.has_bit(current_hartid()) {
        crate::trap::rfence_handler();
    }

    sbi_ret
}

impl rustsbi::Fence for RFence {
    fn remote_fence_i(&self, hart_mask: HartMask) -> SbiRet {
        remote_fence_process(
            RFenceCTX {
                start_addr: 0,
                size: 0,
                asid: 0,
                vmid: 0,
                op: RFenceType::FenceI,
            },
            hart_mask,
        )
    }

    fn remote_sfence_vma(&self, hart_mask: HartMask, start_addr: usize, size: usize) -> SbiRet {
        // TODO: return SBI_ERR_INVALID_ADDRESS, when start_addr or size is not valid.
        let flush_size = match start_addr.checked_add(size) {
            None => usize::MAX,
            Some(_) => size,
        };
        remote_fence_process(
            RFenceCTX {
                start_addr,
                size: flush_size,
                asid: 0,
                vmid: 0,
                op: RFenceType::SFenceVma,
            },
            hart_mask,
        )
    }

    fn remote_sfence_vma_asid(
        &self,
        hart_mask: HartMask,
        start_addr: usize,
        size: usize,
        asid: usize,
    ) -> SbiRet {
        // TODO: return SBI_ERR_INVALID_ADDRESS, when start_addr or size is not valid.
        let flush_size = match start_addr.checked_add(size) {
            None => usize::MAX,
            Some(_) => size,
        };
        remote_fence_process(
            RFenceCTX {
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

pub fn hart_mask_clear(hart_mask: HartMask, hart_id: usize) -> HartMask {
    let (mask, mask_base) = hart_mask.into_inner();
    if mask_base == usize::MAX {
        return HartMask::from_mask_base(mask & (!(1 << hart_id)), 0);
    }
    let Some(idx) = hart_id.checked_sub(mask_base) else {
        return hart_mask;
    };
    if idx >= usize::BITS as usize {
        return hart_mask;
    }
    HartMask::from_mask_base(mask & (!(1 << hart_id)), mask_base)
}
