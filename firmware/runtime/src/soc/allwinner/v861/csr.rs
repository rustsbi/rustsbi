//! V861 C907 custom-CSR and reset-cache operations.
//!
//! The API remains visible on non-RISC-V targets for static analysis, but an
//! attempted hardware operation on such a target panics.
//!
//! # References
//!
//! - Reference implementation: [OpenSBI D1 cache setup](https://github.com/riscv-software-src/opensbi/blob/3593a5facc4c6938b90429a6973ba9ee21fc5899/platform/generic/allwinner/sun20i-d1.c)
//!   — shared C9xx status/cache registers and cache-invalidate encoding.

use super::AllwinnerV861Soc;
use crate::csr::{Readable, Writable, native_bit_ops, native_registers};
use spin::Once;

static BOOT_CACHE: Once<C907CacheState> = Once::new();

native_registers! {
    ordered;
    read {}
    write {
        ExtendedStatus: usize = 0x7c0;
        CacheControl: usize = 0x7c1;
        L2Control: usize = 0x7c3;
        Hint: usize = 0x7c5;
    }
    write_only {
        CacheOperation: usize = 0x7c2;
        SmpControl: usize = 0x7f3;
    }
}

native_bit_ops! { ordered; set { CacheControl; } clear {} }

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
#[repr(usize)]
pub(super) enum CacheCommand {
    InvalidateAll = 0x70013,
}

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
pub(super) const SMP_COHERENCY_ENABLE: usize = 1;

/// Cache policy captured on the boot C907 and restored on reset harts.
struct C907CacheState {
    l2: usize,
    status: usize,
    hint: usize,
    cache: usize,
}

impl C907CacheState {
    fn read() -> Self {
        Self {
            l2: L2Control::read().expect("C907 L2 control read failed"),
            status: ExtendedStatus::read().expect("C907 extended status read failed"),
            hint: Hint::read().expect("C907 hint register read failed"),
            cache: CacheControl::read().expect("C907 cache control read failed"),
        }
    }

    /// Restores this policy on a hardware-reset C907 after joining coherency.
    fn restore(&self) {
        L2Control::write(self.l2).expect("C907 L2 control write failed");
        ExtendedStatus::write(self.status).expect("C907 extended status write failed");
        Hint::write(self.hint).expect("C907 hint register write failed");
        CacheControl::write(self.cache).expect("C907 cache control write failed");
    }
}

impl AllwinnerV861Soc {
    pub(super) fn save_boot_cache(self) {
        BOOT_CACHE.call_once(|| {
            const CACHE_CONTROL_ENABLE: usize = (1 << 24) | (1 << 12);
            CacheControl::set_bits(CACHE_CONTROL_ENABLE);
            C907CacheState::read()
        });
    }
}

pub(super) fn restore_boot_cache() {
    BOOT_CACHE
        .get()
        .expect("BUG: C907 cache policy must be saved before hart release")
        .restore();
}
