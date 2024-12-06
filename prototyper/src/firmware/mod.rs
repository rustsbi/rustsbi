#[cfg(not(feature = "payload"))]
pub mod dynamic;
#[cfg(feature = "payload")]
pub mod payload;

use core::arch::asm;
use core::ops::Range;
use riscv::register::mstatus;

pub struct BootInfo {
    pub next_address: usize,
    pub mpp: mstatus::MPP,
}

pub struct BootHart {
    pub fdt_address: usize,
    pub is_boot_hart: bool,
}

#[naked]
#[link_section = ".rodata.fdt"]
#[repr(align(16))]
#[cfg(feature = "fdt")]
pub unsafe extern "C" fn raw_fdt() {
    asm!(
        concat!(".incbin \"", env!("PROTOTYPER_FDT_PATH"), "\""),
        options(noreturn)
    );
}

#[inline]
#[cfg(feature = "fdt")]
fn get_fdt_address() -> usize {
    raw_fdt as usize
}

#[cfg(not(feature = "payload"))]
pub use dynamic::{get_boot_info, is_boot_hart};
#[cfg(feature = "payload")]
pub use payload::{get_boot_info, is_boot_hart};

/// Gets boot hart information based on opaque and nonstandard_a2 parameters.
///
/// Returns a BootHart struct containing FDT address and whether this is the boot hart.
#[allow(unused_mut, unused_assignments)]
pub fn get_boot_hart(opaque: usize, nonstandard_a2: usize) -> BootHart {
    let is_boot_hart = is_boot_hart(nonstandard_a2);

    let mut fdt_address = opaque;

    #[cfg(feature = "fdt")]
    {
        fdt_address = get_fdt_address();
    }

    BootHart {
        fdt_address,
        is_boot_hart,
    }
}

pub fn set_pmp(memory_range: &Range<usize>) {
    unsafe {
        // [0..memory_range.start] RW
        // [memory_range.start..sbi_start] RWX
        // [sbi_start..sbi_end] NONE
        // [sbi_end..memory_range.end] RWX
        // [memory_range.end..INF] RW
        use riscv::register::*;
        let mut sbi_start_address: usize;
        let mut sbi_end_address: usize;
        asm!("la {}, sbi_start", out(reg) sbi_start_address, options(nomem));
        asm!("la {}, sbi_end", out(reg) sbi_end_address, options(nomem));
        pmpcfg0::set_pmp(0, Range::OFF, Permission::NONE, false);
        pmpaddr0::write(0);
        pmpcfg0::set_pmp(1, Range::TOR, Permission::RW, false);
        pmpaddr1::write(memory_range.start >> 2);
        pmpcfg0::set_pmp(2, Range::TOR, Permission::RWX, false);
        pmpaddr2::write(sbi_start_address >> 2);
        pmpcfg0::set_pmp(3, Range::TOR, Permission::NONE, false);
        pmpaddr3::write(sbi_end_address >> 2);
        pmpcfg0::set_pmp(4, Range::TOR, Permission::RWX, false);
        pmpaddr4::write(memory_range.end >> 2);
        pmpcfg0::set_pmp(5, Range::TOR, Permission::RW, false);
        pmpaddr5::write(usize::MAX >> 2);
    }
}
