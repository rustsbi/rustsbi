//! Safe boot entry points over the global platform state.

use core::ops::Range;
use core::sync::atomic::Ordering;

use super::{IS_K1_PLATFORM, PLATFORM, READY};
use crate::riscv::spacemit_k1;

/// Initializes the board from the device tree and runs the SoC-specific
/// early initialization.
pub fn init_board(fdt_address: usize) {
    unsafe {
        PLATFORM.init(fdt_address);
        PLATFORM.print_board_info();
    }

    if IS_K1_PLATFORM.load(Ordering::Acquire) {
        // Configure ML2SETUP for the boot hart
        spacemit_k1::cold_boot_allowed(crate::riscv::current_hartid());

        unsafe {
            // Use the SBI link address as the warmboot entry
            let warmboot_addr = crate::cfg::SBI_LINK_START_ADDRESS as u64;
            spacemit_k1::early_init(true, warmboot_addr);
        }
        info!("SpacemiT K1: early init done (MSETUP + CCI-550)");
    }
}

/// Runs the SoC-specific per-hart setup for secondary harts.
pub fn secondary_hart_init() {
    if IS_K1_PLATFORM.load(Ordering::Acquire) {
        spacemit_k1::cold_boot_allowed(crate::riscv::current_hartid());
    }
}

/// Spins until the boot hart has finished platform initialization.
pub fn wait_until_ready() {
    while !READY.load(Ordering::Acquire) {
        core::hint::spin_loop()
    }
}

/// Returns the board's memory range (set during `Platform::init`).
pub fn memory_range() -> Range<usize> {
    unsafe { PLATFORM.info.memory_range.as_ref().unwrap().clone() }
}

/// Reconciles the enabled-CPU table with the per-hart privilege checks.
pub fn refresh_enabled_cpus() {
    unsafe {
        PLATFORM.sbi_cpu_init_with_feature();
    }
}
