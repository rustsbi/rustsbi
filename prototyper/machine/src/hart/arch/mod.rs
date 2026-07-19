//! Target selection for hart-local architectural operations.

#[cfg(all(test, not(any(target_arch = "riscv32", target_arch = "riscv64"))))]
mod model;
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
mod riscv;
#[cfg(all(not(test), not(any(target_arch = "riscv32", target_arch = "riscv64"))))]
mod unsupported;

#[cfg(all(test, not(any(target_arch = "riscv32", target_arch = "riscv64"))))]
pub(super) use model::*;
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
pub(super) use riscv::*;
#[cfg(all(not(test), not(any(target_arch = "riscv32", target_arch = "riscv64"))))]
pub(super) use unsupported::*;
