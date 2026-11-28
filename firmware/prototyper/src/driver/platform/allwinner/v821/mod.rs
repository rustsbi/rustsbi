//! V821 platform initialization and on-chip devices.

#![forbid(unsafe_code)]

pub(crate) mod a27l2;
mod control;
pub(crate) mod usb_dma_bypass;

use runtime::{
    PlatformView, Result,
    memory::MemoryRegistry,
    soc::allwinner::v821::{A27L2CacheControl, AllwinnerV821Soc},
};

use crate::{
    driver::platform::{Board, TimerDesc},
    firmware::BootHart,
    sbi::{pmu::SbiPmu, vendor::VendorDevices},
};

pub(crate) struct V821Board {
    pub(crate) boot: BootHart<AllwinnerV821Soc>,
    pub(crate) cache: A27L2CacheControl,
}

impl Board for V821Board {
    fn early_init(
        &mut self,
        memory: &mut MemoryRegistry,
        timer: Option<&TimerDesc>,
    ) -> Result<Option<u64>> {
        let alias = self.initialize_boot_devices(memory, timer)?;
        Ok(Some(alias.offset()))
    }

    fn init(
        &mut self,
        platform: &PlatformView<'_>,
        memory: &mut MemoryRegistry,
        _pmu: &mut Option<SbiPmu>,
    ) -> Result<Option<VendorDevices>> {
        VendorDevices::init(self.cache, platform, memory).map(Some)
    }
}
