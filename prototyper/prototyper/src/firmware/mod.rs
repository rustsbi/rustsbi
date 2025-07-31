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
use core::{ops::Range, usize};
use riscv::register::mstatus;

use pmpm::{PmpSlice, get_pmp_entry, set_pmp_entry};

pub struct BootInfo {
    pub next_address: usize,
    pub mpp: mstatus::MPP,
}

pub struct BootHart {
    pub fdt_address: usize,
    pub is_boot_hart: bool,
}

#[unsafe(naked)]
#[unsafe(link_section = ".fdt")]
#[repr(align(16))]
#[cfg(feature = "fdt")]
pub extern "C" fn raw_fdt() {
    naked_asm!(concat!(".incbin \"", env!("PROTOTYPER_FDT_PATH"), "\""),)
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
        // [0..memory_range.start] RWX
        // [memory_range.start..sbi_start] RWX
        // [sbi_start..sbi_rodata_start] NONE
        // [sbi_rodata_start..sbi_rodata_end] NONE
        // [sbi_rodata_end..sbi_end] NONE
        // [sbi_end..memory_range.end] RWX
        // [memory_range.end..INF] RWX
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

        // pmpcfg0::set_pmp(0, Range::OFF, Permission::NONE, false);
        // pmpaddr0::write(0);
        // pmpcfg0::set_pmp(1, Range::TOR, Permission::RWX, false);
        // pmpaddr1::write(memory_range.start >> 2);
        // pmpcfg0::set_pmp(2, Range::TOR, Permission::RWX, false);
        // pmpaddr2::write(SBI_START_ADDRESS >> 2);
        // pmpcfg0::set_pmp(3, Range::TOR, Permission::NONE, false);
        // pmpaddr3::write(RODATA_START_ADDRESS >> 2);
        // pmpcfg0::set_pmp(4, Range::TOR, Permission::NONE, false);
        // pmpaddr4::write(RODATA_END_ADDRESS >> 2);
        // pmpcfg0::set_pmp(5, Range::TOR, Permission::NONE, false);
        // pmpaddr5::write(SBI_END_ADDRESS >> 2);
        // pmpcfg0::set_pmp(6, Range::TOR, Permission::RWX, false);
        // pmpaddr6::write(memory_range.end >> 2);
        // pmpcfg0::set_pmp(7, Range::TOR, Permission::RWX, false);
        // pmpaddr7::write(usize::MAX >> 2);
        set_pmp_entry(0, PmpSlice::new(0, 0, 0), Range::OFF, Permission::NONE);

        set_pmp_entry(
            1,
            PmpSlice::new(0, memory_range.start, 0),
            Range::TOR,
            Permission::RWX,
        );

        set_pmp_entry(
            2,
            PmpSlice::new(0, SBI_START_ADDRESS, 0),
            Range::TOR,
            Permission::RWX,
        );
        set_pmp_entry(
            3,
            PmpSlice::new(0, RODATA_START_ADDRESS, 0),
            Range::TOR,
            Permission::NONE,
        );
        set_pmp_entry(
            4,
            PmpSlice::new(0, RODATA_END_ADDRESS, 0),
            Range::TOR,
            Permission::NONE,
        );
        set_pmp_entry(
            5,
            PmpSlice::new(0, SBI_END_ADDRESS, 0),
            Range::TOR,
            Permission::NONE,
        );
        set_pmp_entry(
            6,
            PmpSlice::new(0, memory_range.end, 0),
            Range::TOR,
            Permission::RWX,
        );
        set_pmp_entry(
            7,
            PmpSlice::new(0, usize::MAX, 0),
            Range::TOR,
            Permission::RWX,
        );

        set_pmp_entry(
            8,
            PmpSlice::new(memory_range.end - memory_range.start, memory_range.start, 0),
            Range::NAPOT,
            Permission::RWX,
        );
    }
}

pub fn log_pmp_cfg(memory_range: &Range<usize>) {
    info!("PMP Configuration");
    info!(
        "{:<10} {:<10} {:<15} {:<30}",
        "PMP", "Range", "Permission", "Address"
    );
    for i in 0..16 {
        let (slice, config) = get_pmp_entry(i);
        info!(
            "{:<10} 0x{:<10b} 0x{:<15b} 0x{:08x}+0x{:08x}",
            format_args!("PMP {}", i),
            config.range as u8,
            config.permission as u8,
            slice.lo(),
            slice.size()
        );
    }
}
