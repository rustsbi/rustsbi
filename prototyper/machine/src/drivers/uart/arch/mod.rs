//! Target selection for UART device ordering.

#[cfg(all(test, not(any(target_arch = "riscv32", target_arch = "riscv64"))))]
mod model;
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
mod riscv;
#[cfg(all(not(test), not(any(target_arch = "riscv32", target_arch = "riscv64"))))]
mod unsupported;

#[cfg(all(test, not(any(target_arch = "riscv32", target_arch = "riscv64"))))]
pub(super) use model::io_fence;
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
pub(super) use riscv::io_fence;
#[cfg(all(not(test), not(any(target_arch = "riscv32", target_arch = "riscv64"))))]
pub(super) use unsupported::io_fence;
