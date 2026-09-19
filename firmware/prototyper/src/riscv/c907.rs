//! RISC-V entry adapter for a C907 released from hardware reset.

/// Enters a reset C907 without a bootloader register envelope.
///
/// # Safety
///
/// V861 hardware may branch here only after the boot hart publishes stacks,
/// platform state, the saved cache policy, and the pending HSM start request.
/// The assembly uses no stack before Runtime restores coherency.
#[unsafe(naked)]
unsafe extern "C" fn reset_entry() -> ! {
    core::arch::naked_asm!(
        ".balign 4",
        "la a0, {initialize}",
        "tail {entry}",
        initialize = sym crate::driver::allwinner::v861::initialize_secondary,
        entry = sym runtime::boot::c907_reset_entry,
    );
}

/// Returns the entry address programmed into a C907 reset vector.
pub(crate) fn reset_entry_address() -> usize {
    reset_entry as *const () as usize
}
