//! Firmware callbacks for platform-specific Runtime reset entries.

use runtime::soc::{allwinner::v861::AllwinnerV861Soc, spacemit::k1::SpacemitK1Soc};

/// Connects K1 hardware reset to this firmware's secondary-hart policy.
///
/// # Safety
///
/// Enter only under [`SpacemitK1Soc::reset_entry`]'s hardware-reset conditions.
#[unsafe(naked)]
pub(crate) unsafe extern "C" fn k1_reset_entry() -> ! {
    core::arch::naked_asm!(
        ".balign 4",
        ".option push",
        ".option norelax",
        "lla a0, {initialize}",
        ".option pop",
        "tail {entry}",
        initialize = sym initialize_k1,
        entry = sym SpacemitK1Soc::reset_entry,
    )
}

/// Connects V861 C907 reset to the saved cache policy and secondary-hart policy.
///
/// # Safety
///
/// Enter only under [`AllwinnerV861Soc::reset_entry`]'s hardware-reset conditions.
#[unsafe(naked)]
pub(crate) unsafe extern "C" fn c907_reset_entry() -> ! {
    core::arch::naked_asm!(
        ".balign 4",
        ".option push",
        ".option norelax",
        "lla a0, {initialize}",
        ".option pop",
        "tail {entry}",
        initialize = sym crate::driver::platform::allwinner::v861::initialize_secondary,
        entry = sym AllwinnerV861Soc::reset_entry,
    )
}

extern "C" fn initialize_k1() {
    if let Err(error) = crate::secondary_hart(None) {
        error!("Failed to initialize the hart: {error}");
        crate::fail::stop();
    }
}
