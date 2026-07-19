//! Closed per-hart trap delegation.

mod policy;
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
mod riscv;

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
pub(crate) use riscv::prepare;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InterruptError {
    Readback,
}
