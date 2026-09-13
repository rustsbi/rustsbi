pub mod allwinner_v861;
pub mod csr;
pub mod spacemit_k1;

/// Returns the current hart (hardware thread) ID.
#[inline]
pub fn current_hartid() -> usize {
    riscv::register::mhartid::read()
}
