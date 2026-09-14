//! Entry for a K1 hart released from hardware reset after platform publication.

/// Enters firmware on K1 without a previous-stage register handoff.
///
/// # Safety
/// The hart must be a K1. The boot hart must have initialized stacks, services and coherency
/// before hardware releases this DT-enabled hart. HSM owns its startup data.
#[unsafe(naked)]
pub(crate) unsafe extern "C" fn warm_entry() -> ! {
    core::arch::naked_asm!(
        ".balign 4",
        "la a0, {initialize}",
        "tail {entry}",
        initialize = sym initialize,
        entry = sym runtime::boot::k1_warm_entry,
    )
}

extern "C" fn initialize() {
    crate::secondary_hart(None);
}
