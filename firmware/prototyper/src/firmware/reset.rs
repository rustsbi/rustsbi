//! Firmware callbacks for platform-specific Runtime reset entries.

use runtime::soc::{allwinner::v861::AllwinnerV861Soc, spacemit::k1::SpacemitK1Soc};

/// Connects K1 hardware reset to this firmware's secondary-hart policy.
///
/// # Safety
///
/// Enter only under [`SpacemitK1Soc::reset_entry`]'s hardware-reset conditions.
#[unsafe(naked)]
pub(crate) unsafe extern "C" fn spacemit_k1_reset_entry() -> ! {
    core::arch::naked_asm!(
        ".balign 4",
        ".option push",
        ".option norelax",
        "lla a0, {initialize}",
        ".option pop",
        "tail {entry}",
        initialize = sym initialize_spacemit_k1,
        entry = sym SpacemitK1Soc::reset_entry,
    )
}

/// Connects V861 C907 reset to the saved cache policy and secondary-hart policy.
///
/// # Safety
///
/// Enter only under [`AllwinnerV861Soc::reset_entry`]'s hardware-reset conditions.
#[unsafe(naked)]
pub(crate) unsafe extern "C" fn thead_c907_reset_entry() -> ! {
    core::arch::naked_asm!(
        ".balign 4",
        ".option push",
        ".option norelax",
        "lla a0, {initialize}",
        ".option pop",
        "tail {entry}",
        initialize = sym initialize_thead_c907,
        entry = sym AllwinnerV861Soc::reset_entry,
    )
}

/// Initializes a K1 secondary hart after hardware reset.
///
/// # Safety
///
/// Enter through [`SpacemitK1Soc::reset_entry`] with the platform published
/// and the startup conditions of [`crate::secondary_hart`] in effect.
unsafe extern "C" fn initialize_spacemit_k1() {
    // SAFETY: Runtime reset entry prepared the hart and installed trap handling.
    if let Err(error) = unsafe { crate::secondary_hart(None) } {
        error!("Failed to initialize the hart: {error}");
        crate::fail::stop();
    }
}

/// Restores the boot C907's cache policy and initializes the reset hart.
///
/// # Safety
///
/// Enter through [`AllwinnerV861Soc::reset_entry`] after cache invalidation
/// and entry into coherency, under the startup conditions of [`crate::secondary_hart`].
unsafe extern "C" fn initialize_thead_c907() {
    let result = crate::driver::platform::allwinner::v861::cache_state().and_then(|cache| {
        // SAFETY:
        // 1. This is a C907 on the board that published the cache policy.
        //    Its reset entry invalidated the caches and enabled coherency.
        // 2. Runtime installed trap recovery with interrupts masked before
        //    calling this initializer on the newly released hart.
        unsafe {
            cache.restore();
            crate::secondary_hart(None)
        }
    });
    if let Err(error) = result {
        error!("Failed to initialize the hart: {error}");
        crate::fail::stop();
    }
}
