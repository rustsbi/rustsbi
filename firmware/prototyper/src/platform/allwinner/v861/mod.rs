//! Allwinner V861 platform support.
//!
//! The C907 hart-release device lives in [`devices`]; `early_init` binds it
//! and `final_init` publishes the hardware wakeup capability.

mod c907;
mod devices;

use alloc::boxed::Box;
use runtime::{
    Error, hart::HartWake, memory::MemoryRegistry, soc::allwinner::v861::AllwinnerV861Soc,
};

use crate::platform::{Platform, PlatformDriver};

use devices::V861HartRelease;

pub(crate) use devices::initialize_secondary;

/// V861 platform driver, selected by the SoC capability.
pub(crate) struct V861Driver;

pub(crate) static V861: V861Driver = V861Driver;

impl PlatformDriver for V861Driver {
    fn matches(&self, platform: &runtime::PlatformView<'_>) -> runtime::Result<bool> {
        Ok(platform.soc::<AllwinnerV861Soc>()?.is_some())
    }

    fn create(&self) -> Box<dyn Platform> {
        Box::new(V861Platform::default())
    }
}

#[derive(Default)]
struct V861Platform {
    wake: Option<V861HartRelease>,
}

impl Platform for V861Platform {
    fn early_init(
        &mut self,
        _cold_boot: bool,
        platform: &runtime::PlatformView<'_>,
        _info: &mut crate::info::PlatformInfo,
        memory: &mut MemoryRegistry,
    ) -> runtime::Result<()> {
        let soc = platform
            .soc::<AllwinnerV861Soc>()?
            .ok_or(Error::InvalidArgs)?;
        self.wake = Some(V861HartRelease::bind(soc, memory)?);
        Ok(())
    }

    fn final_init(
        &mut self,
        _cold_boot: bool,
        _platform: &runtime::PlatformView<'_>,
        _info: &mut crate::info::PlatformInfo,
        _memory: &mut MemoryRegistry,
    ) -> runtime::Result<()> {
        if let Some(wake) = self.wake.take() {
            let wake: Box<dyn HartWake> = Box::new(wake);
            let wake: &'static dyn HartWake = Box::leak(wake);
            runtime::hart::install_wakeup(Some(wake));
        }
        crate::sbi::pmu::set_default(crate::sbi::pmu::SbiPmu::new);
        Ok(())
    }
}
