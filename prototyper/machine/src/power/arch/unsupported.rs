//! Fail-stop backend for accidental non-RISC-V use.

pub(crate) fn mask_local_interrupts() {}

pub(crate) fn halt() -> ! {
    panic!("machine termination requires a RISC-V target")
}
