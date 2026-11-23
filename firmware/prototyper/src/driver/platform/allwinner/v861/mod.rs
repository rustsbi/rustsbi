//! V861 C907 hart-release device.

mod device;

pub(crate) use device::initialize_secondary;

use runtime::{
    PlatformView, Result,
    memory::{MemoryRegistry, SupervisorMemory},
    soc::allwinner::v861::AllwinnerV861Soc,
};

use crate::{
    driver::platform::Board,
    sbi::{pmu::SbiPmu, vendor::Vendor},
};

impl Board for AllwinnerV861Soc {
    fn init(
        &self,
        _platform: &PlatformView<'_>,
        memory: &mut MemoryRegistry,
        _supervisor_memory: &'static SupervisorMemory,
        pmu: &mut Option<SbiPmu>,
    ) -> Result<Option<Vendor>> {
        device::init(*self, memory)?;
        pmu.get_or_insert_default();
        Ok(None)
    }
}
