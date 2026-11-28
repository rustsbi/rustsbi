#![forbid(unsafe_code)]

//! Board selection and shared platform initialization.
//!
//! The boot hart reads the device tree and publishes the initialized services.

pub(crate) mod allwinner;
mod boot;
mod harts;
mod interrupts;
mod report;
pub(crate) mod spacemit_k1;

pub(crate) mod qemu_aplic;

use runtime::{PlatformView, Result, memory::MemoryRegistry};

use crate::sbi::{pmu::SbiPmu, vendor::VendorDevices};

pub(crate) use boot::init;
pub use boot::wait_until_ready;
pub(crate) use interrupts::TimerDesc;
pub(crate) use spacemit_k1::secondary_soc;

/// Board-specific startup operations performed on the boot hart.
pub(crate) trait Board {
    /// Prepares hardware before console and interrupt devices are bound.
    ///
    /// Returns the offset to a second mapping of firmware memory if PMP must protect it.
    fn early_init(
        &mut self,
        _memory: &mut MemoryRegistry,
        _timer: Option<&TimerDesc>,
    ) -> Result<Option<u64>> {
        Ok(None)
    }

    /// Initializes board devices after the shared devices are ready and returns
    /// any hardware used by vendor SBI extensions.
    fn init(
        &mut self,
        _platform: &PlatformView<'_>,
        _memory: &mut MemoryRegistry,
        _pmu: &mut Option<SbiPmu>,
    ) -> Result<Option<VendorDevices>> {
        Ok(None)
    }
}

pub(crate) struct Generic;

impl Board for Generic {}
