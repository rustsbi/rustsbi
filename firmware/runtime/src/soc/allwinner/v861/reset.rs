//! C907 reset entry: restore coherent memory before selecting a Rust stack.

use crate::boot::{ResetEntry, ResetEntryAlreadyRegistered};
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
use crate::csr::Csr;

use super::{AllwinnerV861Soc, csr};

impl AllwinnerV861Soc {
    /// Saves the boot cache policy and registers the V861 reset initializer.
    ///
    /// Publish platform services and pending HSM work before powering on a hart.
    /// Runtime restores this cache policy before calling the initializer, which
    /// must activate Runtime traps before returning.
    pub fn register_reset_entry(
        self,
        initialize: fn(),
    ) -> Result<ResetEntry, ResetEntryAlreadyRegistered> {
        self.save_boot_cache();
        ResetEntry::register(reset_entry, initialize)
    }
}

/// Enters a reset C907 without using shared memory until caches are coherent.
///
/// # Safety
///
/// Only a V861 C907 hart may enter in M-mode. The boot hart must have saved its
/// cache policy and published the initializer, services and pending HSM request.
#[cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), unsafe(naked))]
unsafe extern "C" fn reset_entry() -> ! {
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    core::arch::naked_asm!(
        ".balign 4",
        "csrw mie, zero",
        "csrci mstatus, 8",
        "lla t0, {fail}",
        "csrw mtvec, t0",
        "csrw mscratch, zero",
        "li t0, {invalidate_all}",
        "csrw {cache_operation}, t0",
        "li t0, {smp_enable}",
        "csrw {smp_control}, t0",
        "fence rw, rw",
        "call {locate}",
        "tail {initialize}",
        fail = sym crate::boot::fail_stop,
        locate = sym crate::boot::locate_stack,
        initialize = sym initialize_reset_hart,
        invalidate_all = const csr::CacheCommand::InvalidateAll as usize,
        cache_operation = const csr::CacheOperation::NUMBER,
        smp_enable = const csr::SMP_COHERENCY_ENABLE,
        smp_control = const csr::SmpControl::NUMBER,
    );
    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    unimplemented!("C907 hardware reset requires a RISC-V target");
}

extern "C" fn initialize_reset_hart() -> ! {
    csr::restore_boot_cache();
    crate::boot::reset::initialize_reset_hart()
}
