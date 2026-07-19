//! Fail-stop entry point for accidental non-RISC-V use.

use alloc::boxed::Box;

use super::BootInfo;

/// Rejects a next-stage transition outside the RISC-V machine environment.
pub fn enter_next_stage(_boot: BootInfo, _handler: Box<dyn crate::TrapHandler>) -> ! {
    panic!("next-stage entry requires a RISC-V target")
}
