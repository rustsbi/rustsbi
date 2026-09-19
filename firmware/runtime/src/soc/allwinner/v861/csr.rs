//! Safe accessors for the V861 C907 custom CSRs.
//!
//! The API remains visible on non-RISC-V targets for static analysis, but an
//! attempted hardware operation on such a target panics.
//!
//! # References
//!
//! - Reference implementation: [OpenSBI D1 cache setup](https://github.com/riscv-software-src/opensbi/blob/3593a5facc4c6938b90429a6973ba9ee21fc5899/platform/generic/allwinner/sun20i-d1.c)
//!   — C907 CSR numbers and cache-policy bits.

use super::AllwinnerV861Soc;

/// Cache policy captured on the boot C907 and restored on reset harts.
pub struct C907CacheState {
    l2: usize,
    status: usize,
    hint: usize,
    cache: usize,
}

impl C907CacheState {
    fn read() -> Self {
        arch::read()
    }

    /// Restores this policy on a hardware-reset C907.
    pub fn restore(&self) {
        arch::restore(self);
    }
}

impl AllwinnerV861Soc {
    /// Enables firmware C907 cache controls and captures the loader policy.
    pub fn initialize_c907_cache(self) -> C907CacheState {
        arch::enable_cache_controls();
        C907CacheState::read()
    }
}

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
mod arch {
    use super::C907CacheState;

    const L2_CONTROL_CSR: usize = 0x7c3;
    const EXTENDED_STATUS_CSR: usize = 0x7c0;
    const HINT_CSR: usize = 0x7c5;
    const CACHE_CONTROL_CSR: usize = 0x7c1;
    const CACHE_CONTROL_ENABLE: usize = (1 << 24) | (1 << 12);

    pub(super) fn read() -> C907CacheState {
        let (l2, status, hint, cache);
        // SAFETY: the public interface is reachable only through a V861 SoC
        // capability, and the firmware invokes it in machine mode.
        unsafe {
            core::arch::asm!(
                "csrr {l2}, {l2_csr}", "csrr {status}, {status_csr}",
                "csrr {hint}, {hint_csr}", "csrr {cache}, {cache_csr}",
                l2_csr = const L2_CONTROL_CSR,
                status_csr = const EXTENDED_STATUS_CSR,
                hint_csr = const HINT_CSR,
                cache_csr = const CACHE_CONTROL_CSR,
                l2 = out(reg) l2,
                status = out(reg) status,
                hint = out(reg) hint,
                cache = out(reg) cache,
            );
        }
        C907CacheState {
            l2,
            status,
            hint,
            cache,
        }
    }

    pub(super) fn restore(state: &C907CacheState) {
        // SAFETY: the reset hart has invalidated its caches and joined
        // coherency before its first shared-memory access.
        unsafe {
            core::arch::asm!(
                "csrw {l2_csr}, {l2}", "csrw {status_csr}, {status}",
                "csrw {hint_csr}, {hint}", "csrw {cache_csr}, {cache}",
                l2_csr = const L2_CONTROL_CSR,
                status_csr = const EXTENDED_STATUS_CSR,
                hint_csr = const HINT_CSR,
                cache_csr = const CACHE_CONTROL_CSR,
                l2 = in(reg) state.l2,
                status = in(reg) state.status,
                hint = in(reg) state.hint,
                cache = in(reg) state.cache,
            );
        }
    }

    pub(super) fn enable_cache_controls() {
        // SAFETY: the public interface requires a V861 C907 capability and
        // the firmware invokes it in machine mode.
        unsafe {
            core::arch::asm!(
                "csrs {cache_csr}, {mask}",
                cache_csr = const CACHE_CONTROL_CSR,
                mask = in(reg) CACHE_CONTROL_ENABLE,
                options(nomem, nostack),
            );
        }
    }
}

#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
mod arch {
    use super::C907CacheState;

    pub(super) fn read() -> C907CacheState {
        unsupported()
    }

    pub(super) fn restore(state: &C907CacheState) {
        let _ = (state.l2, state.status, state.hint, state.cache);
        unsupported()
    }

    pub(super) fn enable_cache_controls() {
        unsupported()
    }

    #[cold]
    #[track_caller]
    fn unsupported() -> ! {
        panic!("V861 custom CSRs require a RISC-V target")
    }
}
