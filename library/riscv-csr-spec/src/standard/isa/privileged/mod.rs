//! Privileged architectural CSR specifications.
//!
//! At M, S and VS privilege levels, the [indirect CSR interface](https://docs.riscv.org/reference/isa/v20260120/priv/indirect-csr.html#indirect-csr)
//! pairs a selector with data aliases. Keep the selector stable until the data
//! access completes. [IMSIC interrupt files](https://docs.riscv.org/reference/aia/v1.0/IMSIC.html)
//! use this interface.

pub mod debug;
pub mod hypervisor;
pub mod machine;
pub mod supervisor;
pub mod virtual_supervisor;
