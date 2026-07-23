//! Terminal RISC-V machine-state operations.

mod riscv;

pub(crate) use riscv::{halt, mask_local_interrupts};
