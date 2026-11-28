//! Boot-hart access to board-specific PMA and cache controls.

use core::marker::PhantomData;
use runtime::{
    Error, PlatformDescription, Result,
    soc::{
        allwinner::{
            v821::{AllwinnerV821Soc, NoncacheableAlias},
            v861::{AllwinnerV861Soc, C907CacheState},
        },
        spacemit::k1::SpacemitK1Soc,
    },
};

use crate::driver::platform::{self, allwinner::v821::V821Board};

/// Permits one attempt to initialize the selected board's CSRs on the boot hart.
pub(crate) struct BootHart<T> {
    pending: Option<T>,
    _hart_local: PhantomData<*mut ()>,
}

impl<T: Copy> BootHart<T> {
    fn new(hardware: T) -> Self {
        Self {
            pending: Some(hardware),
            _hart_local: PhantomData,
        }
    }

    pub(crate) fn hardware(&self) -> Result<T> {
        self.pending.ok_or(Error::AlreadyInitialized)
    }

    fn take(&mut self) -> Result<T> {
        self.pending.take().ok_or(Error::AlreadyInitialized)
    }
}

impl BootHart<AllwinnerV821Soc> {
    pub(crate) fn initialize_noncacheable_alias(&mut self) -> Result<NoncacheableAlias> {
        let soc = self.take()?;
        // SAFETY: The boot contract assigns writable PMA entry 15 and its
        // configuration word to this hart while live mappings remain valid.
        unsafe { soc.initialize_noncacheable_alias() }
    }
}

impl BootHart<SpacemitK1Soc> {
    pub(crate) fn initialize_hart(&mut self) -> Result<()> {
        let soc = self.take()?;
        // SAFETY: The boot hart owns the K1 cache, predictor and ECC setup.
        // The boot contract excludes secondary harts until cluster coherency is ready.
        unsafe { soc.initialize_hart() };
        Ok(())
    }
}

impl BootHart<AllwinnerV861Soc> {
    pub(crate) fn initialize_c907_cache(&mut self) -> Result<C907CacheState> {
        let soc = self.take()?;
        // SAFETY: Startup preserves the loader's live memory and coherency
        // requirements while enabling cache controls on this C907.
        Ok(unsafe { soc.initialize_c907_cache() })
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
/// 2. Call at most once per boot, even if initialization fails. Run on the
///    boot hart in M-mode with `MPRV` clear and local interrupts disabled.
///    Keep secondary harts excluded until initialization completes.
/// 3. Own the board-specific cache controls, including the K1 predictor and
///    ECC controls. Keep all live firmware and loader memory accessible and coherent.
/// 4. On V821, own writable PMA entry 15 and its configuration word before
///    drivers use the noncacheable mirror. Cache commands on every participating
///    hart require M-mode physical addressing and an enabled Andes CCTL interface.
///    Whole-cache writeback must preserve live memory, including buffers shared with devices.
pub(crate) unsafe fn init_platform(description: PlatformDescription) -> Result<usize> {
    if let Some(soc) = description.inspect(|view| view.soc::<AllwinnerV821Soc>())? {
        // SAFETY: The caller establishes the V821 addressing and writeback
        // requirements on every hart that will use the published handle.
        let cache = unsafe { soc.cache_control()? };
        return platform::init(
            description,
            &mut V821Board {
                boot: BootHart::new(soc),
                cache,
            },
        );
    }
    if let Some(soc) = description.inspect(|view| view.soc::<SpacemitK1Soc>())? {
        return platform::init(description, &mut BootHart::new(soc));
    }
    if let Some(soc) = description.inspect(|view| view.soc::<AllwinnerV861Soc>())? {
        return platform::init(description, &mut BootHart::new(soc));
    }
    platform::init(description, &mut platform::Generic)
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
