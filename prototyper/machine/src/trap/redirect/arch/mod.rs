//! Target selection for supervisor trap CSR commits.

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
mod riscv;
#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
mod unsupported;

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
pub(in crate::trap) use riscv::{read_supervisor_vector, write_supervisor_trap};
#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
pub(in crate::trap) use unsupported::{read_supervisor_vector, write_supervisor_trap};
