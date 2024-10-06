use aclint::SifiveClint;
use core::{
    ptr::null_mut,
    sync::atomic::{
        AtomicPtr,
        Ordering::{Relaxed, Release},
    },
};
use rustsbi::SbiRet;

use crate::hsm::remote_hsm;
use crate::riscv_spec::{current_hartid, stimecmp};
use crate::trap_stack::ROOT_STACK;

pub(crate) static SIFIVECLINT: AtomicPtr<SifiveClint> = AtomicPtr::new(null_mut());
pub(crate) const IPI_TYPE_SSOFT: u8 = 1 << 0;
pub(crate) const IPI_TYPE_FENCE: u8 = 1 << 1;

pub(crate) fn init(base: usize) {
    SIFIVECLINT.store(base as _, Release);
}

#[inline]
pub fn clear() {
    let hart_id = current_hartid();
    loop {
        if let Some(clint) = unsafe { SIFIVECLINT.load(Relaxed).as_ref() } {
            clint.clear_msip(hart_id);
            clint.write_mtimecmp(hart_id, u64::MAX);
            break;
        } else {
            continue;
        }
    }
}

#[inline]
pub fn clear_msip() {
    loop {
        if let Some(clint) = unsafe { SIFIVECLINT.load(Relaxed).as_ref() } {
            clint.clear_msip(current_hartid());
            break;
        } else {
            continue;
        }
    }
}

#[inline]
pub fn set_msip(hart_id: usize) {
    loop {
        if let Some(clint) = unsafe { SIFIVECLINT.load(Relaxed).as_ref() } {
            clint.set_msip(hart_id);
            break;
        } else {
            continue;
        }
    }
}

pub struct ClintDevice<'a> {
    pub clint: &'a AtomicPtr<SifiveClint>,
    pub max_hart_id: usize,
}

impl<'a> ClintDevice<'a> {
    pub fn new(clint: &'a AtomicPtr<SifiveClint>, max_hart_id: usize) -> Self {
        Self { clint, max_hart_id }
    }
}

impl<'a> ClintDevice<'a> {
    #[inline]
    pub fn get_time(&self) -> usize {
        unsafe { (*self.clint.load(Relaxed)).read_mtime() as usize }
    }

    #[inline]
    pub fn get_timeh(&self) -> usize {
        unsafe { ((*self.clint.load(Relaxed)).read_mtime() >> 32) as usize }
    }
}

impl<'a> rustsbi::Timer for ClintDevice<'a> {
    #[inline]
    fn set_timer(&self, stime_value: u64) {
        unsafe {
            // TODO: 添加CPU拓展探测机制，补充无Sstc拓展时的定时器设置
            stimecmp::set(stime_value);
            riscv::register::mie::set_mtimer();
        }
    }
}

impl<'a> rustsbi::Ipi for ClintDevice<'a> {
    #[inline]
    fn send_ipi(&self, hart_mask: rustsbi::HartMask) -> SbiRet {
        for hart_id in 0..=self.max_hart_id {
            if hart_mask.has_bit(hart_id) && remote_hsm(hart_id).unwrap().allow_ipi() {
                let old_ipi_type = set_ipi_type(hart_id, crate::clint::IPI_TYPE_SSOFT);
                if old_ipi_type == 0 {
                    unsafe {
                        (*self.clint.load(Relaxed)).set_msip(hart_id);
                    }
                }
            }
        }
        SbiRet::success(0)
    }
}

impl<'a> ClintDevice<'a> {
    #[inline]
    pub fn send_ipi_by_fence(
        &self,
        hart_mask: rustsbi::HartMask,
        ctx: crate::rfence::RFenceCTX,
    ) -> SbiRet {
        for hart_id in 0..=self.max_hart_id {
            if hart_mask.has_bit(hart_id) && remote_hsm(hart_id).unwrap().allow_ipi() {
                crate::rfence::remote_rfence(hart_id).unwrap().set(ctx);
                crate::rfence::local_rfence().add();
                if hart_id == current_hartid() {
                    continue;
                }
                let old_ipi_type = set_ipi_type(hart_id, crate::clint::IPI_TYPE_FENCE);
                if old_ipi_type == 0 {
                    unsafe {
                        (*self.clint.load(Relaxed)).set_msip(hart_id);
                    }
                }
            }
        }
        while crate::rfence::local_rfence().is_zero() == false {
            crate::trap::rfence_signle_handler();
        }
        SbiRet::success(0)
    }
}

pub fn set_ipi_type(hart_id: usize, event_id: u8) -> u8 {
    unsafe {
        ROOT_STACK
            .get_unchecked_mut(hart_id)
            .hart_context()
            .ipi_type
            .fetch_or(event_id, Relaxed)
    }
}

pub fn get_and_reset_ipi_type() -> u8 {
    unsafe {
        ROOT_STACK
            .get_unchecked_mut(current_hartid())
            .hart_context()
            .ipi_type
            .swap(0, Relaxed)
    }
}
