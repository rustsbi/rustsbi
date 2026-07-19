//! Target selection for the concrete APLIC register transport.

#[cfg(all(test, not(any(target_arch = "riscv32", target_arch = "riscv64"))))]
mod model;
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
mod riscv;
#[cfg(all(not(test), not(any(target_arch = "riscv32", target_arch = "riscv64"))))]
mod unsupported;

#[cfg(all(test, not(any(target_arch = "riscv32", target_arch = "riscv64"))))]
pub(super) use model::configure_device;
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
pub(super) use riscv::configure_device;
#[cfg(all(not(test), not(any(target_arch = "riscv32", target_arch = "riscv64"))))]
pub(super) use unsupported::configure_device;
