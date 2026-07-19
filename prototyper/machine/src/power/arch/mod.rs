//! Target selection for terminal machine-state operations.

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
mod riscv;
#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
mod unsupported;

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
pub(crate) use riscv::{halt, mask_local_interrupts};
#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
pub(crate) use unsupported::{halt, mask_local_interrupts};
