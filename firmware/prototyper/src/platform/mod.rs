#![deny(unsafe_code)]

//! Target operations.
//!
//! A [`Platform`] is selected once from the root device tree and provides the
//! OpenSBI-style `early_init`/`final_init` hooks. Target code lives under
//! `allwinner/` and `spacemit/`; shared discovered facts live in `info`.

mod allwinner;
mod spacemit;

use alloc::boxed::Box;
use runtime::{PlatformView, Result, memory::MemoryRegistry};

use crate::info::PlatformInfo;

/// Target operations for one boot target.
///
/// These are the two OpenSBI platform hooks: `early_init` runs before the
/// subsystem initialization sequence, and `final_init` runs after it.
pub(crate) trait Platform: Send + Sync {
    /// Target initialization before device and SBI subsystem setup.
    fn early_init(
        &mut self,
        _cold_boot: bool,
        _platform: &PlatformView<'_>,
        _info: &mut PlatformInfo,
        _memory: &mut MemoryRegistry,
    ) -> Result<()> {
        Ok(())
    }

    /// Target initialization after subsystem setup.
    fn final_init(
        &mut self,
        _cold_boot: bool,
        _platform: &PlatformView<'_>,
        _info: &mut PlatformInfo,
        _memory: &mut MemoryRegistry,
    ) -> Result<()> {
        Ok(())
    }
}

/// Root-compatible matcher that creates one [`Platform`] instance.
pub(crate) trait PlatformDriver: Send + Sync {
    fn matches(&self, platform: &PlatformView<'_>) -> Result<bool>;

    fn create(&self) -> Box<dyn Platform>;
}

/// Platform-driver table. Exactly one entry is selected per boot.
pub(crate) struct Platforms {
    entries: &'static [&'static dyn PlatformDriver],
}

impl Platforms {
    pub(crate) const fn new(entries: &'static [&'static dyn PlatformDriver]) -> Self {
        Self { entries }
    }

    pub(crate) fn select(&self, platform: &PlatformView<'_>) -> Result<Option<Box<dyn Platform>>> {
        for driver in self.entries {
            if driver.matches(platform)? {
                return Ok(Some(driver.create()));
            }
        }
        Ok(None)
    }
}

pub(crate) static PLATFORMS: Platforms = Platforms::new(&[
    &allwinner::v821::V821,
    &allwinner::v861::V861,
    &spacemit::k1::K1,
]);

pub(crate) fn select(platform: &PlatformView<'_>) -> Result<Option<Box<dyn Platform>>> {
    PLATFORMS.select(platform)
}

/// Fallback platform for plain FDT machines.
pub(crate) struct Generic;

impl Platform for Generic {}
