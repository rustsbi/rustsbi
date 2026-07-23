//! RISC-V supervisor trap CSR commits.

mod riscv;

pub(in crate::trap) use riscv::{read_supervisor_vector, write_supervisor_trap};
