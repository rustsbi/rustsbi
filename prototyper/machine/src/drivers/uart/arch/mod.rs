//! RISC-V UART device ordering.

mod riscv;

pub(super) use riscv::io_fence;
