//! Fail-stop backend for accidental non-RISC-V use outside unit tests.

pub(in crate::timer) fn current_hart_id() -> usize {
    panic!("timer architecture operations require a RISC-V target")
}
