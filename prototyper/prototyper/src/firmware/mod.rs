cfg_if::cfg_if! {
    if #[cfg(feature = "payload")] {
        pub mod payload;
        pub use payload::{get_boot_info, is_boot_hart};
    } else if #[cfg(feature = "jump")] {
        pub mod jump;
        pub use jump::{get_boot_info, is_boot_hart};
    } else {
        pub mod dynamic;
        pub use dynamic::{get_boot_info, is_boot_hart};
    }
}

#[allow(unused)]
use core::arch::{asm, naked_asm};
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
#[unsafe(link_section = ".rodata.fdt")]
#[repr(align(16))]
#[cfg(feature = "fdt")]
pub extern "C" fn raw_fdt() {
    unsafe { naked_asm!(concat!(".incbin \"", env!("PROTOTYPER_FDT_PATH"), "\""),) }
}

#[inline]
#[cfg(feature = "fdt")]
fn get_fdt_address() -> usize {
    raw_fdt as usize
}

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

static mut SBI_START_ADDRESS: usize = 0;
static mut SBI_END_ADDRESS: usize = 0;
static mut RODATA_START_ADDRESS: usize = 0;
static mut RODATA_END_ADDRESS: usize = 0;

pub fn set_pmp(memory_range: &Range<usize>) {
    unsafe {
        // [0..memory_range.start] RW
        // [memory_range.start..sbi_start] RWX
        // [sbi_start..sbi_rodata_start] NONE
        // [sbi_rodata_start..sbi_rodata_end] NONE
        // [sbi_rodata_end..sbi_end] NONE
        // [sbi_end..memory_range.end] RWX
        // [memory_range.end..INF] RW
        use riscv::register::*;

        asm!("la {}, sbi_start", out(reg) SBI_START_ADDRESS, options(nomem));
        asm!("la {}, sbi_end", out(reg) SBI_END_ADDRESS, options(nomem));
        asm!("la {}, sbi_rodata_start", out(reg) RODATA_START_ADDRESS, options(nomem));
        asm!("la {}, sbi_rodata_end", out(reg) RODATA_END_ADDRESS, options(nomem));

        assert_eq!(memory_range.start & 0x3, 0);
        assert_eq!(memory_range.end & 0x3, 0);
        assert_eq!(SBI_START_ADDRESS & 0x3, 0);
        assert_eq!(SBI_END_ADDRESS & 0x3, 0);
        assert_eq!(RODATA_START_ADDRESS & 0x3, 0);
        assert_eq!(RODATA_END_ADDRESS & 0x3, 0);

        pmpcfg0::set_pmp(0, Range::OFF, Permission::NONE, false);
        pmpaddr0::write(0);
        pmpcfg0::set_pmp(1, Range::TOR, Permission::RWX, false);
        pmpaddr1::write(memory_range.start >> 2);
        pmpcfg0::set_pmp(2, Range::TOR, Permission::RWX, false);
        pmpaddr2::write(SBI_START_ADDRESS >> 2);
        pmpcfg0::set_pmp(3, Range::TOR, Permission::NONE, false);
        pmpaddr3::write(RODATA_START_ADDRESS >> 2);
        pmpcfg0::set_pmp(4, Range::TOR, Permission::NONE, false);
        pmpaddr4::write(RODATA_END_ADDRESS >> 2);
        pmpcfg0::set_pmp(5, Range::TOR, Permission::NONE, false);
        pmpaddr5::write(SBI_END_ADDRESS >> 2);
        pmpcfg0::set_pmp(6, Range::TOR, Permission::RWX, false);
        pmpaddr6::write(memory_range.end >> 2);
        pmpcfg0::set_pmp(7, Range::TOR, Permission::RWX, false);
        pmpaddr7::write(usize::MAX >> 2);
    }
}

pub fn log_pmp_cfg(memory_range: &Range<usize>) {
    unsafe {
        info!("PMP Configuration");

        info!(
            "{:<10} {:<10} {:<15} {:<30}",
            "PMP", "Range", "Permission", "Address"
        );

        info!("{:<10} {:<10} {:<15} 0x{:08x}", "PMP 0:", "OFF", "NONE", 0);
        info!(
            "{:<10} {:<10} {:<15} 0x{:08x} - 0x{:08x}",
            "PMP 1-2:", "TOR", "RWX/RWX", memory_range.start, SBI_START_ADDRESS
        );
        info!(
            "{:<10} {:<10} {:<15} 0x{:08x} - 0x{:08x} - 0x{:08x}",
            "PMP 3-5:",
            "TOR",
            "NONE/NONE",
            RODATA_START_ADDRESS,
            RODATA_END_ADDRESS,
            SBI_END_ADDRESS
        );
        info!(
            "{:<10} {:<10} {:<15} 0x{:08x}",
            "PMP 6:", "TOR", "RWX", memory_range.end
        );
        info!(
            "{:<10} {:<10} {:<15} 0x{:08x}",
            "PMP 7:",
            "TOR",
            "RW",
            usize::MAX
        );
    }
}
