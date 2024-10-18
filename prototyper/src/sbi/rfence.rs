use rustsbi::{HartMask, SbiRet};
use spin::Mutex;

use crate::board::SBI_IMPL;
use crate::riscv_spec::current_hartid;
use crate::sbi::fifo::{Fifo, FifoError};
use crate::sbi::trap;
use crate::sbi::trap_stack::ROOT_STACK;

use core::sync::atomic::{AtomicU32, Ordering};

pub(crate) struct RFenceCell {
    queue: Mutex<Fifo<(RFenceCTX, usize)>>, // (ctx, source_hart_id)
    wait_sync_count: AtomicU32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct RFenceCTX {
    pub start_addr: usize,
    pub size: usize,
    pub asid: usize,
    pub vmid: usize,
    pub op: RFenceType,
}

#[allow(unused)]
#[derive(Clone, Copy, Debug)]
pub enum RFenceType {
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
    pub fn local(&self) -> LocalRFenceCell<'_> {
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
pub(crate) fn local_rfence() -> Option<LocalRFenceCell<'static>> {
    unsafe {
        ROOT_STACK
            .get_mut(current_hartid())
            .map(|x| x.hart_context().rfence.local())
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
    pub fn is_sync(&self) -> bool {
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
                Ok(_) => break,
                Err(FifoError::Full) => {
                    trap::rfence_signle_handler();
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
                Ok(_) => break,
                Err(FifoError::Full) => {
                    trap::rfence_signle_handler();
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
pub(crate) struct SbiRFence;

fn remote_fence_process(rfence_ctx: RFenceCTX, hart_mask: HartMask) -> SbiRet {
    let sbi_ret = unsafe { SBI_IMPL.assume_init_mut() }
        .ipi
        .as_ref()
        .unwrap()
        .send_ipi_by_fence(hart_mask, rfence_ctx);

    sbi_ret
}

impl rustsbi::Fence for SbiRFence {
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
