//! V861 cache-policy snapshots and hardware-reset preparation.
//!
//! CSR definitions and hardware operations come from `csr::vendor::thead`.

use super::AllwinnerV861Soc;
use csr::vendor::thead::{
    c907::{CacheControl, Hint, L2Control},
    machine::ExtendedStatus,
};

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
        // SAFETY: The SoC capability identifies C907 and firmware runs in
        // M-mode before releasing secondary harts.
        unsafe { CacheControl::set_bits(CacheControl::V861_ENABLE) };
        C907CacheState::read()
    }
}
