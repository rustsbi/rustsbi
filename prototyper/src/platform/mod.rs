#[cfg(not(feature = "payload"))]
pub mod dynamic;
#[cfg(feature = "payload")]
pub mod payload;

use core::arch::asm;
use riscv::register::mstatus;

pub struct BootInfo {
    pub next_address: usize,
    pub mpp: mstatus::MPP,
}

pub struct BootHart {
    pub fdt_address: usize,
    pub is_boot_hart: bool,
}

#[cfg(not(feature = "payload"))]
pub use dynamic::{get_boot_hart, get_boot_info};
#[cfg(feature = "payload")]
pub use payload::{get_boot_hart, get_boot_info};

pub fn set_pmp() {
    // TODO: PMP configuration needs to be obtained through the memory range in the device tree
    unsafe {
        use riscv::register::*;
        let mut sbi_start_address: usize;
        let mut sbi_end_address: usize;
        asm!("la {}, sbi_start", out(reg) sbi_start_address, options(nomem));
        asm!("la {}, sbi_end", out(reg) sbi_end_address, options(nomem));
        pmpcfg0::set_pmp(0, Range::OFF, Permission::NONE, false);
        pmpaddr0::write(0);
        pmpcfg0::set_pmp(1, Range::TOR, Permission::RWX, false);
        pmpaddr1::write(sbi_start_address);
        pmpcfg0::set_pmp(2, Range::TOR, Permission::NONE, false);
        pmpaddr2::write(sbi_end_address);
        pmpcfg0::set_pmp(3, Range::TOR, Permission::RWX, false);
        pmpaddr3::write(usize::MAX >> 2);
    }
}
