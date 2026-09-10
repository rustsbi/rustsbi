//! Entry for a K1 hart released from hardware reset after platform publication.

use crate::sbi::{trap::boot::boot, trap_stack};

/// Enters firmware on K1 without a previous-stage register handoff.
///
/// # Safety
/// The hart must be a K1. The boot hart must have initialized stacks, services and coherency
/// before hardware releases this DT-enabled hart. HSM owns its startup data.
#[unsafe(naked)]
pub(crate) unsafe extern "C" fn warm_entry() -> ! {
    core::arch::naked_asm!(
        ".balign 4",
        "csrw mie, zero",
        "call {prepare_hart}",
        "call {locate_stack}",
        "call {initialize}",
        "csrw mscratch, sp",
        "j {boot}",
        locate_stack = sym trap_stack::locate,
        prepare_hart = sym runtime::SpacemitK1Registers::prepare_warm_hart,
        initialize = sym initialize,
        boot = sym boot,
    )
}

extern "C" fn initialize() {
    crate::secondary_hart(None);
}
