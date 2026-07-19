//! Fail-stop backend for accidental non-RISC-V IMSIC use.

use super::super::ImsicError;

fn unavailable() -> ! {
    panic!("IMSIC operations require a RISC-V target")
}

pub(in crate::drivers::imsic) fn initialize_current_file(_: u16, _: u16) -> Result<(), ImsicError> {
    unavailable()
}

pub(in crate::drivers::imsic) fn current_hart_id() -> usize {
    unavailable()
}

pub(in crate::drivers::imsic) fn claim_current_file(_: usize) {
    unavailable()
}

pub(in crate::drivers::imsic) fn device_fence() {
    unavailable()
}
