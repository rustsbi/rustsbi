//! Build limits selected independently for firmware and host validation.

#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
mod host;
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
mod riscv;

#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
pub(crate) use host::*;
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
pub(crate) use riscv::*;
