use core::sync::atomic::{AtomicPtr, Ordering::Relaxed};
use rustsbi::SbiRet;

use crate::board::SBI_IMPL;
use crate::riscv_spec::{current_hartid, stimecmp};
use crate::sbi::extensions::{hart_extension_probe, Extension};
use crate::sbi::hsm::remote_hsm;
use crate::sbi::rfence;
use crate::sbi::trap;
use crate::sbi::trap_stack::ROOT_STACK;

pub(crate) const IPI_TYPE_SSOFT: u8 = 1 << 0;
pub(crate) const IPI_TYPE_FENCE: u8 = 1 << 1;

#[allow(unused)]
pub trait IpiDevice {
    fn read_mtime(&self) -> u64;
    fn write_mtime(&self, val: u64);
    fn read_mtimecmp(&self, hart_idx: usize) -> u64;
    fn write_mtimecmp(&self, hart_idx: usize, val: u64);
    fn read_msip(&self, hart_idx: usize) -> bool;
    fn set_msip(&self, hart_idx: usize);
    fn clear_msip(&self, hart_idx: usize);
}

pub struct SbiIpi<'a, T: IpiDevice> {
    pub ipi_dev: &'a AtomicPtr<T>,
    pub max_hart_id: usize,
}

impl<'a, T: IpiDevice> rustsbi::Timer for SbiIpi<'a, T> {
    #[inline]
    fn set_timer(&self, stime_value: u64) {
        if hart_extension_probe(current_hartid(), Extension::Sstc) {
            stimecmp::set(stime_value);
            unsafe {
                riscv::register::mie::set_mtimer();
            }
        } else {
            self.write_mtimecmp(current_hartid(), stime_value);
            unsafe {
                riscv::register::mip::clear_stimer();
                riscv::register::mie::set_mtimer();
            }
        }
    }
}

impl<'a, T: IpiDevice> rustsbi::Ipi for SbiIpi<'a, T> {
    #[inline]
    fn send_ipi(&self, hart_mask: rustsbi::HartMask) -> SbiRet {
        for hart_id in 0..=self.max_hart_id {
            if hart_mask.has_bit(hart_id) && remote_hsm(hart_id).unwrap().allow_ipi() {
                let old_ipi_type = set_ipi_type(hart_id, IPI_TYPE_SSOFT);
                if old_ipi_type == 0 {
                    unsafe {
                        (*self.ipi_dev.load(Relaxed)).set_msip(hart_id);
                    }
                }
            }
        }
        SbiRet::success(0)
    }
}

impl<'a, T: IpiDevice> SbiIpi<'a, T> {
    pub fn new(ipi_dev: &'a AtomicPtr<T>, max_hart_id: usize) -> Self {
        Self {
            ipi_dev,
            max_hart_id,
        }
    }

    pub fn send_ipi_by_fence(
        &self,
        hart_mask: rustsbi::HartMask,
        ctx: rfence::RFenceCTX,
    ) -> SbiRet {
        for hart_id in 0..=self.max_hart_id {
            if hart_mask.has_bit(hart_id) && remote_hsm(hart_id).unwrap().allow_ipi() {
                rfence::remote_rfence(hart_id).unwrap().set(ctx);
                rfence::local_rfence().unwrap().add();
                if hart_id == current_hartid() {
                    continue;
                }
                let old_ipi_type = set_ipi_type(hart_id, IPI_TYPE_FENCE);
                if old_ipi_type == 0 {
                    unsafe {
                        (*self.ipi_dev.load(Relaxed)).set_msip(hart_id);
                    }
                }
            }
        }
        while !rfence::local_rfence().unwrap().is_sync() {
            trap::rfence_single_handler();
        }
        SbiRet::success(0)
    }

    #[inline]
    pub fn get_time(&self) -> usize {
        unsafe { (*self.ipi_dev.load(Relaxed)).read_mtime() as usize }
    }

    #[inline]
    pub fn get_timeh(&self) -> usize {
        unsafe { ((*self.ipi_dev.load(Relaxed)).read_mtime() >> 32) as usize }
    }

    #[inline]
    pub fn set_msip(&self, hart_idx: usize) {
        unsafe { (*self.ipi_dev.load(Relaxed)).set_msip(hart_idx) }
    }

    #[inline]
    pub fn clear_msip(&self, hart_idx: usize) {
        unsafe { (*self.ipi_dev.load(Relaxed)).clear_msip(hart_idx) }
    }

    #[inline]
    pub fn write_mtimecmp(&self, hart_idx: usize, val: u64) {
        unsafe { (*self.ipi_dev.load(Relaxed)).write_mtimecmp(hart_idx, val) }
    }

    #[inline]
    pub fn clear(&self) {
        let hart_id = current_hartid();
        unsafe {
            (*self.ipi_dev.load(Relaxed)).clear_msip(hart_id);
            (*self.ipi_dev.load(Relaxed)).write_mtimecmp(hart_id, u64::MAX);
        }
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

pub fn clear_msip() {
    unsafe { SBI_IMPL.assume_init_ref() }
        .ipi
        .as_ref()
        .unwrap()
        .clear_msip(current_hartid())
}

pub fn clear_mtime() {
    unsafe { SBI_IMPL.assume_init_ref() }
        .ipi
        .as_ref()
        .unwrap()
        .write_mtimecmp(current_hartid(), u64::MAX)
}

pub fn clear_all() {
    unsafe { SBI_IMPL.assume_init_ref() }
        .ipi
        .as_ref()
        .unwrap()
        .clear();
}
