//! Fail-stop backend for accidental non-RISC-V UART use.

pub(in crate::drivers::uart) fn io_fence() {
    panic!("UART device access requires a RISC-V target")
}
