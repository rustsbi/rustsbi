use aclint::SifiveClint;
use core::arch::asm;
use xuantie_riscv::peripheral::clint::THeadClint;

use crate::sbi::ipi::IpiDevice;
pub(crate) const SIFIVE_CLINT_COMPATIBLE: [&str; 1] = ["riscv,clint0"];
pub(crate) const THEAD_CLINT_COMPATIBLE: [&str; 1] = ["thead,c900-clint"];

#[doc(hidden)]
#[allow(unused)]
#[derive(Clone, Copy, Debug)]
pub enum MachineClintType {
    SiFiveClint,
    TheadClint,
}

/// For SiFive Clint
pub struct SifiveClintWrap {
    inner: *const SifiveClint,
}

impl SifiveClintWrap {
    pub fn new(base: usize) -> Self {
        Self {
            inner: base as *const SifiveClint,
        }
    }
}

impl IpiDevice for SifiveClintWrap {
    #[inline(always)]
    fn read_mtime(&self) -> u64 {
        unsafe { (*self.inner).read_mtime() }
    }

    #[inline(always)]
    fn write_mtime(&self, val: u64) {
        unsafe { (*self.inner).write_mtime(val) }
    }

    #[inline(always)]
    fn read_mtimecmp(&self, hart_idx: usize) -> u64 {
        unsafe { (*self.inner).read_mtimecmp(hart_idx) }
    }

    #[inline(always)]
    fn write_mtimecmp(&self, hart_idx: usize, val: u64) {
        unsafe { (*self.inner).write_mtimecmp(hart_idx, val) }
    }

    #[inline(always)]
    fn read_msip(&self, hart_idx: usize) -> bool {
        unsafe { (*self.inner).read_msip(hart_idx) }
    }

    #[inline(always)]
    fn set_msip(&self, hart_idx: usize) {
        unsafe { (*self.inner).set_msip(hart_idx) }
    }

    #[inline(always)]
    fn clear_msip(&self, hart_idx: usize) {
        unsafe { (*self.inner).clear_msip(hart_idx) }
    }
}

/// For T-Head Clint
pub struct THeadClintWrap {
    inner: *const THeadClint,
}

impl THeadClintWrap {
    pub fn new(base: usize) -> Self {
        Self {
            inner: base as *const THeadClint,
        }
    }
}

impl IpiDevice for THeadClintWrap {
    #[inline(always)]
    fn read_mtime(&self) -> u64 {
        unsafe {
            let mut mtime: u64 = 0;
            asm!(
                "rdtime {}",
                inout(reg) mtime,
            );
            mtime
        }
    }

    #[inline(always)]
    fn write_mtime(&self, _val: u64) {
        unimplemented!()
    }

    #[inline(always)]
    fn read_mtimecmp(&self, hart_idx: usize) -> u64 {
        unsafe { (*self.inner).read_mtimecmp(hart_idx) }
    }

    #[inline(always)]
    fn write_mtimecmp(&self, hart_idx: usize, val: u64) {
        unsafe { (*self.inner).write_mtimecmp(hart_idx, val) }
    }

    #[inline(always)]
    fn read_msip(&self, hart_idx: usize) -> bool {
        unsafe { (*self.inner).read_msip(hart_idx) }
    }

    #[inline(always)]
    fn set_msip(&self, hart_idx: usize) {
        unsafe { (*self.inner).set_msip(hart_idx) }
    }

    #[inline(always)]
    fn clear_msip(&self, hart_idx: usize) {
        unsafe { (*self.inner).clear_msip(hart_idx) }
    }
}
