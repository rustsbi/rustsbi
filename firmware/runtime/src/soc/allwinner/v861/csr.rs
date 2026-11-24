//! V861 cache-policy snapshots and hardware-reset preparation.
//!
//! CSR definitions and hardware operations come from `csr::vendor::thead`.

use super::AllwinnerV861Soc;
use csr::vendor::thead::machine::{CacheControl, ExtendedStatus, Hint, L2Control};

/// Cache policy captured on the boot C907 and restored on reset harts.
pub struct C907CacheState {
    l2: L2Control,
    status: ExtendedStatus,
    hint: Hint,
    cache: CacheControl,
}

impl C907CacheState {
    fn read() -> Self {
        Self {
            l2: L2Control::read(),
            status: ExtendedStatus::read(),
            hint: Hint::read(),
            cache: CacheControl::read(),
        }
    }

    /// Restores this policy on a hardware-reset C907.
    pub fn restore(&self) {
        // SAFETY: A reset C907 has invalidated its caches and joined coherency
        // before restoring the boot C907's saved configuration.
        unsafe {
            L2Control::write(self.l2);
            ExtendedStatus::restore(self.status);
            Hint::write(self.hint);
            CacheControl::write(self.cache);
        }
    }
}

impl AllwinnerV861Soc {
    /// Enables firmware C907 cache controls and captures the loader policy.
    pub fn initialize_c907_cache(self) -> C907CacheState {
        // Opaque `mhcr` enable bits from the V861 OpenSBI platform source:
        // https://github.com/YuzukiHD/opensbi/blob/c1ea219a901ff309e88e99c4b4e66ef9a55548de/platform/generic/allwinner/sun252i-v861.c
        const V861_CACHE_ENABLE: usize = (1 << 24) | (1 << 12);

        // SAFETY: The SoC capability identifies C907 and firmware runs in
        // M-mode before releasing secondary harts.
        unsafe { CacheControl::set_bits(V861_CACHE_ENABLE) };
        C907CacheState::read()
    }

    /// Starts a C907 released by the V861 reset controller.
    ///
    /// # Safety
    ///
    /// Enter in M-mode with MPRV clear after the boot hart publishes topology,
    /// services, and the saved cache policy. `initialize` must restore that
    /// policy and complete per-hart firmware setup before returning.
    #[cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), unsafe(naked))]
    pub unsafe extern "C" fn reset_entry(initialize: extern "C" fn()) -> ! {
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        core::arch::naked_asm!(
            "mv a1, a0",
            ".option push",
            ".option norelax",
            "lla a0, {prepare}",
            ".option pop",
            "tail {entry}",
            prepare = sym prepare_reset_hart,
            entry = sym crate::boot::reset_hart_entry,
        );
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        {
            let _ = initialize;
            unimplemented!("C907 reset entry requires a RISC-V target");
        }
    }
}

// Invalidate caches and join coherency before Runtime reads the hart table.
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
#[unsafe(naked)]
unsafe extern "C" fn prepare_reset_hart() {
    core::arch::naked_asm!(
        "li t0, {invalidate}",
        "csrw {cache_operation}, t0",
        "li t0, {enable}",
        "csrw {snoop_control}, t0",
        "fence rw, rw",
        "ret",
        invalidate = const csr::vendor::thead::machine::CacheOperation::INVALIDATE_ALL,
        cache_operation = const csr::vendor::thead::machine::CacheOperation::NUMBER,
        enable = const csr::vendor::thead::machine::SnoopControl::ENABLE,
        snoop_control = const csr::vendor::thead::machine::SnoopControl::NUMBER,
    );
}
