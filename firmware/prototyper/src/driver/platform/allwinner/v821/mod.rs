//! V821 platform initialization and on-chip devices.

#![forbid(unsafe_code)]

pub(crate) mod a27l2;
mod control;
pub(crate) mod usb_dma_bypass;

use runtime::{
    PlatformView, Result,
    memory::{MemoryRegistry, SupervisorMemory},
    soc::allwinner::v821::{AllwinnerV821Soc, V821Csr},
};

use crate::{
    driver::platform::{Board, TimerDesc},
    firmware::BootHart,
    sbi::{pmu::SbiPmu, vendor::Vendor},
};

pub(crate) struct V821Board {
    pub(crate) soc: AllwinnerV821Soc,
    pub(crate) csr: BootHart<V821Csr>,
}

impl Board for V821Board {
    fn early_init(
        &self,
        memory: &mut MemoryRegistry,
        timer: Option<&TimerDesc>,
    ) -> Result<Option<u64>> {
        let alias = self.initialize_boot_devices(memory, timer)?;
        Ok(Some(alias.offset()))
    }

    fn init(
        &self,
        platform: &PlatformView<'_>,
        memory: &mut MemoryRegistry,
        supervisor_memory: &'static SupervisorMemory,
        _pmu: &mut Option<SbiPmu>,
    ) -> Result<Option<Vendor>> {
        Vendor::init(self.csr.hardware(), platform, memory, supervisor_memory).map(Some)
    }
}
