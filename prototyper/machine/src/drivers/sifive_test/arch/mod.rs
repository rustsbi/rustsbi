//! Target selection for SiFive test-register ordering.

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
mod riscv;
#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
mod unsupported;

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
pub(super) use riscv::device_fence;
#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
pub(super) use unsupported::device_fence;
