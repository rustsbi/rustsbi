//! RISC-V entry adapter for a C907 released from hardware reset.

use runtime::boot;

use super::device::initialize_secondary;

/// Enters a reset C907 without a bootloader register envelope.
///
/// # Safety
///
/// V861 hardware may branch here only after the boot hart publishes stacks,
/// platform state, the saved cache policy, and the pending HSM start request.
#[allow(unsafe_code)]
#[unsafe(naked)]
unsafe extern "C" fn reset_entry() -> ! {
    // The entry uses no stack before Runtime restores coherency.
    core::arch::naked_asm!(
        ".balign 4",
        "la a0, {initialize}",
        "tail {entry}",
        initialize = sym initialize_secondary,
        entry = sym boot::c907_reset_entry,
    );
}

/// Returns the entry address programmed into a C907 reset vector.
pub(super) fn reset_entry_address() -> usize {
    reset_entry as *const () as usize
}
