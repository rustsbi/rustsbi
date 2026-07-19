//! Target selection for CLINT-local CSR and ordering operations.

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
mod riscv;
#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
mod unsupported;

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
pub(super) use riscv::*;
#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
pub(super) use unsupported::*;
