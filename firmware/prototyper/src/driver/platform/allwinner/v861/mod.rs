//! V861 C907 hart-release device.

mod device;

use crate::{
    driver::platform::Board,
    firmware::BootHart,
    sbi::{pmu::SbiPmu, vendor::Vendor},
};
use runtime::{
    PlatformView, Result,
    memory::{MemoryRegistry, SupervisorMemory},
    soc::allwinner::v861::AllwinnerV861Soc,
};

pub(crate) use device::cache_state;

impl Board for BootHart<AllwinnerV861Soc> {
    fn init(
        &self,
        _platform: &PlatformView<'_>,
        memory: &mut MemoryRegistry,
        _supervisor_memory: &'static SupervisorMemory,
        pmu: &mut Option<SbiPmu>,
    ) -> Result<Option<Vendor>> {
        let release = device::V861HartRelease::enable_clock(self.hardware(), memory)?;
        release.publish(self.initialize_c907_cache());
        pmu.get_or_insert_default();
        Ok(None)
    }
}
