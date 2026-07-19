//! Backend for the contained expected-fault instruction window.

#[cfg(all(not(any(target_arch = "riscv32", target_arch = "riscv64")), test))]
mod model;
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
mod riscv;
#[cfg(all(not(any(target_arch = "riscv32", target_arch = "riscv64")), not(test)))]
mod unsupported;

#[cfg(all(not(any(target_arch = "riscv32", target_arch = "riscv64")), test))]
pub(crate) use model::*;
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
pub(crate) use riscv::*;
#[cfg(all(not(any(target_arch = "riscv32", target_arch = "riscv64")), not(test)))]
pub(crate) use unsupported::*;
