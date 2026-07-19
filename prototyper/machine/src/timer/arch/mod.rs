//! Target selection for the timer's hart-local architecture access.

#[cfg(all(test, not(any(target_arch = "riscv32", target_arch = "riscv64"))))]
mod model;
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
mod riscv;
#[cfg(all(not(test), not(any(target_arch = "riscv32", target_arch = "riscv64"))))]
mod unsupported;

#[cfg(all(test, not(any(target_arch = "riscv32", target_arch = "riscv64"))))]
pub(super) use model::current_hart_id;
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
pub(super) use riscv::current_hart_id;
#[cfg(all(not(test), not(any(target_arch = "riscv32", target_arch = "riscv64"))))]
pub(super) use unsupported::current_hart_id;
