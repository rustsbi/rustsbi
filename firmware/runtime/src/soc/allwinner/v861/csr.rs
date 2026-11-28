//! V861 cache-policy snapshots and hardware-reset preparation.

use super::AllwinnerV861Soc;
use csr::{
    Read, ReadMode, Set, Write,
    vendor::thead::{Mccr2, Mcor, Mhcr, Mhint, Msmpr, Mxstatus},
};

/// Cache policy captured on the boot C907 and restored on reset harts.
pub struct C907CacheState {
    l2: usize,
    status: Mxstatus<ReadMode>,
    hint: usize,
    cache: usize,
}

impl C907CacheState {
    /// Restores this policy on a hardware-reset C907.
    ///
    /// # Safety
    ///
    /// Execute in M-mode on the same C907 implementation from which this policy
    /// was captured. The hart must have invalidated caches and joined coherency;
    /// no live lower-mode execution may depend on the outgoing configuration.
    pub unsafe fn restore(&self) {
        // SAFETY: A reset C907 has invalidated its caches and joined coherency
        // before restoring the boot C907's saved configuration.
        unsafe {
            Mccr2::write(self.l2);
            Mxstatus::write(self.status.into_write());
            Mhint::write(self.hint);
            Mhcr::write(self.cache);
        }
    }
}

impl AllwinnerV861Soc {
    /// Enables the V861 cache controls and captures the resulting policy for reset harts.
    ///
    /// # Safety
    ///
    /// Confirm actual V861/C907 hardware and execute in M-mode during boot,
    /// before releasing secondary harts. Exclusively own cache configuration
    /// and preserve the loader's live memory and coherency requirements.
    pub unsafe fn initialize_c907_cache(self) -> C907CacheState {
        // Opaque `mhcr` enable bits from the V861 OpenSBI platform source:
        // https://github.com/YuzukiHD/opensbi/blob/c1ea219a901ff309e88e99c4b4e66ef9a55548de/platform/generic/allwinner/sun252i-v861.c
        const V861_CACHE_ENABLE: usize = (1 << 24) | (1 << 12);

        // SAFETY: The caller confirms actual C907 hardware and owns its cache
        // configuration in M-mode before releasing secondary harts.
        unsafe {
            Mhcr::write(Set(V861_CACHE_ENABLE));
            C907CacheState {
                l2: Mccr2::read_unchecked(),
                status: Mxstatus::read_unchecked(),
                hint: Mhint::read_unchecked(),
                cache: Mhcr::read_unchecked(),
            }
        }
    }

    /// Starts a C907 released by the V861 reset controller.
    ///
    /// # Safety
    ///
    /// Enter in M-mode with MPRV clear after the boot hart publishes topology,
    /// services, and the saved cache policy. `initialize` must restore that
    /// policy and complete per-hart firmware setup before returning.
    #[unsafe(naked)]
    pub unsafe extern "C" fn reset_entry(initialize: unsafe extern "C" fn()) -> ! {
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
    }
}

/// Prepares a reset C907 for its first access to shared firmware memory.
///
/// # Safety
///
/// Enter in M-mode on a V861 C907 with no live dirty cache lines. The boot
/// hart must have enabled the hardware needed to join coherency.
#[unsafe(naked)]
unsafe extern "C" fn prepare_reset_hart() {
    core::arch::naked_asm!(
        "li t0, {invalidate}",
        "csrw {cache_operation}, t0",
        "li t0, {enable}",
        "csrw {snoop_control}, t0",
        "fence rw, rw",
        "ret",
        invalidate = const Mcor::INVALIDATE_ALL,
        cache_operation = const Mcor::NUMBER,
        enable = const Msmpr::ENABLE,
        snoop_control = const Msmpr::NUMBER,
    );
}
