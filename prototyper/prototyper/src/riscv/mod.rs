pub mod csr;

/// Returns the current hart (hardware thread) ID.
#[inline]
pub fn current_hartid() -> usize {
    riscv::register::mhartid::read()
}
