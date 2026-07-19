//! Fail-stop backend for accidental non-RISC-V power-register use.

pub(in crate::drivers::sifive_test) fn device_fence() {
    panic!("SiFive test-register access requires a RISC-V target")
}
