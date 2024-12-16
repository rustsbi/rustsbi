use core::arch::asm;
use aclint::SifiveClint;
use xuantie_riscv::peripheral::clint::THeadClint;

use crate::sbi::ipi::IpiDevice;
pub(crate) const CLINT_COMPATIBLE: [&str; 1] = ["riscv,clint0"];

#[doc(hidden)]
#[allow(unused)]
#[derive(Clone, Copy, Debug)]
pub enum MachineClintType {
    SiFiveClint,
    TheadClint,
}

#[doc(hidden)]
#[allow(unused)]
pub enum MachineClint {
    SiFive(*const SifiveClint),
    THead(*const THeadClint),
}

/// Ipi Device: Sifive Clint
impl IpiDevice for MachineClint {
    #[inline(always)]
    fn read_mtime(&self) -> u64 {
        match self {
            Self::SiFive(sifive_clint) => unsafe { (**sifive_clint).read_mtime() },
            Self::THead(_) => unsafe {
                let mut mtime: u64 = 0;
                asm!(
                    "rdtime {}",
                    inout(reg) mtime,
                );
                mtime
            },
        }
    }

    #[inline(always)]
    fn write_mtime(&self, val: u64) {
        match self {
            Self::SiFive(sifive_clint) => unsafe { (**sifive_clint).write_mtime(val) },
            Self::THead(_) => {
                unimplemented!()
            }
        }
    }

    #[inline(always)]
    fn read_mtimecmp(&self, hart_idx: usize) -> u64 {
        match self {
            Self::SiFive(sifive_clint) => unsafe { (**sifive_clint).read_mtimecmp(hart_idx) },
            Self::THead(thead_clint) => unsafe { (**thead_clint).read_mtimecmp(hart_idx) },
        }
    }

    #[inline(always)]
    fn write_mtimecmp(&self, hart_idx: usize, val: u64) {
        match self {
            Self::SiFive(sifive_clint) => unsafe { (**sifive_clint).write_mtimecmp(hart_idx, val) },
            Self::THead(thead_clint) => unsafe { (**thead_clint).write_mtimecmp(hart_idx, val) },
        }
    }

    #[inline(always)]
    fn read_msip(&self, hart_idx: usize) -> bool {
        match self {
            Self::SiFive(sifive_clint) => unsafe { (**sifive_clint).read_msip(hart_idx) },
            Self::THead(thead_clint) => unsafe { (**thead_clint).read_msip(hart_idx) },
        }
    }

    #[inline(always)]
    fn set_msip(&self, hart_idx: usize) {
        match self {
            Self::SiFive(sifive_clint) => unsafe { (**sifive_clint).set_msip(hart_idx) },
            Self::THead(thead_clint) => unsafe { (**thead_clint).set_msip(hart_idx) },
        }
    }

    #[inline(always)]
    fn clear_msip(&self, hart_idx: usize) {
        match self {
            Self::SiFive(sifive_clint) => unsafe { (**sifive_clint).clear_msip(hart_idx) },
            Self::THead(thead_clint) => unsafe { (**thead_clint).clear_msip(hart_idx) },
        }
    }
}
