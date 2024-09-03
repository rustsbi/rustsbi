use rustsbi::Ipi;
use rustsbi::{HartMask, SbiRet};
use spin::Mutex;

use crate::board::SBI;
use crate::hsm::remote_hsm;
use crate::riscv_spec::current_hartid;
use crate::trap::msoft_rfence_handler;
use crate::trap_stack::NUM_HART_MAX;
use crate::trap_stack::ROOT_STACK;

pub(crate) struct RFenceCell {
    inner: Mutex<Option<RFenceCTX>>,
}

#[repr(C)]
#[derive(Clone, Copy)]
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
            inner: Mutex::new(None),
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
    pub fn clear(&self) {
        *self.0.inner.lock() = None;
    }
    pub fn get(&self) -> Option<RFenceCTX> {
        (*self.0.inner.lock())
    }
    pub fn set(&self, ctx: RFenceCTX) {
        *self.0.inner.lock() = Some(ctx);
    }
}

#[allow(unused)]
impl RemoteRFenceCell<'_> {
    pub fn set(&self, ctx: RFenceCTX) {
        *self.0.inner.lock() = Some(ctx);
    }
}

/// RFENCE
pub(crate) struct RFence;

fn remote_fence_process(rfence_ctx: RFenceCTX, hart_mask: HartMask) -> SbiRet {
    let mut hart_mask_hsm = hart_mask_clear(hart_mask, current_hartid());
    for hart_id in 0..=NUM_HART_MAX {
        if hart_mask.has_bit(hart_id) {
            if remote_hsm(hart_id).unwrap().allow_ipi() {
                remote_rfence(hart_id).unwrap().set(rfence_ctx);
            } else {
                hart_mask_hsm = hart_mask_clear(hart_mask_hsm, hart_id);
            }
        }
    }
    let sbi_ret = unsafe { SBI.assume_init_mut() }
        .clint
        .as_ref()
        .unwrap()
        .send_ipi(hart_mask_hsm);

    if hart_mask.has_bit(current_hartid()) {
        msoft_rfence_handler();
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
