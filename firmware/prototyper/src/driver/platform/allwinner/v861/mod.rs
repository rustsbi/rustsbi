//! V861 C907 hart-release device.

mod device;

use crate::{
    driver::platform::Board,
    firmware::BootHart,
    sbi::{pmu::SbiPmu, vendor::VendorDevices},
};
use runtime::{
    PlatformView, Result, memory::MemoryRegistry, soc::allwinner::v861::AllwinnerV861Soc,
};

pub(crate) use device::cache_state;

impl Board for BootHart<AllwinnerV861Soc> {
    fn init(
        &mut self,
        _platform: &PlatformView<'_>,
        memory: &mut MemoryRegistry,
        pmu: &mut Option<SbiPmu>,
    ) -> Result<Option<VendorDevices>> {
        let release = device::V861HartRelease::enable_clock(self.hardware()?, memory)?;
        release.publish(self.initialize_c907_cache()?);
        pmu.get_or_insert_default();
        Ok(None)
    }
}
