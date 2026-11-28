//! Boot-hart access to board-specific PMA and cache controls.

use core::marker::PhantomData;
use runtime::{
    PlatformDescription, Result,
    soc::{
        allwinner::{
            v821::{AllwinnerV821Soc, NoncacheableAlias, V821Csr},
            v861::{AllwinnerV861Soc, C907CacheState},
        },
        spacemit::k1::SpacemitK1Soc,
    },
};

use crate::driver::platform::{self, allwinner::v821::V821Board};

/// A handle to the selected hardware for use on the boot hart during initialization.
pub(crate) struct BootHart<T> {
    hardware: T,
    _hart_local: PhantomData<*mut ()>,
}

impl<T: Copy> BootHart<T> {
    fn new(hardware: T) -> Self {
        Self {
            hardware,
            _hart_local: PhantomData,
        }
    }

    pub(crate) fn hardware(&self) -> T {
        self.hardware
    }
}

impl BootHart<V821Csr> {
    pub(crate) fn initialize_noncacheable_alias(&self) -> Result<NoncacheableAlias> {
        // SAFETY: init_platform reserves V821 PMA entry 15 for the boot hart.
        // The board configures the alias before publishing any driver that uses it.
        unsafe { self.hardware.initialize_noncacheable_alias() }
    }
}

impl BootHart<SpacemitK1Soc> {
    pub(crate) fn initialize_hart(&self) {
        // SAFETY: Only the confirmed K1 boot hart holds this handle, before
        // the board enables cluster coherency and releases secondary harts.
        unsafe { self.hardware.initialize_hart() };
    }
}

impl BootHart<AllwinnerV861Soc> {
    pub(crate) fn initialize_c907_cache(&self) -> C907CacheState {
        // SAFETY: The boot hart owns the confirmed C907's cache policy before publication.
        unsafe { self.hardware.initialize_c907_cache() }
    }
}

/// Initializes the boot platform and returns the next-stage FDT address.
///
/// On error, secondary harts continue waiting for platform publication.
/// Earlier device writes and registrations remain in effect.
///
/// # Safety
///
/// 1. Supply a description matching the hardware and its physical resources.
/// 2. Run on the boot hart in M-mode with `MPRV` clear and local interrupts disabled.
///    Keep secondary harts excluded until initialization completes.
/// 3. Own the board's cache configuration and, on V821, PMA entry 15.
///    Keep the loader's live mappings valid throughout these changes.
/// 4. On V821, every hart using the published CSR handle must run in M-mode
///    with Andes cache control enabled and mappings that permit cache writeback.
pub(crate) unsafe fn init_platform(description: PlatformDescription) -> Result<usize> {
    if let Some(soc) = description.inspect(|view| view.soc::<AllwinnerV821Soc>())? {
        // SAFETY: The caller confirms V821 CCTL access for every user of this handle.
        let csr = BootHart::new(unsafe { soc.csr()? });
        return platform::init(description, &V821Board { soc, csr });
    }
    if let Some(soc) = description.inspect(|view| view.soc::<SpacemitK1Soc>())? {
        return platform::init(description, &BootHart::new(soc));
    }
    if let Some(soc) = description.inspect(|view| view.soc::<AllwinnerV861Soc>())? {
        return platform::init(description, &BootHart::new(soc));
    }
    platform::init(description, &platform::Generic)
}

/// Completes machine setup for a secondary hart of the published platform.
///
/// # Safety
///
/// Wait for platform publication before calling.
/// Run in M-mode on the secondary hart with local interrupts disabled,
/// while startup still owns its cache, predictor and ECC configuration.
pub(crate) unsafe fn initialize_secondary_hart() {
    if let Some(soc) = platform::secondary_soc() {
        // SAFETY: The boot hart confirmed the K1 and published cluster coherency;
        // the caller owns this secondary hart's startup configuration.
        unsafe { soc.initialize_hart() };
    }
}
