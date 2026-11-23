//! V821 platform initialization and on-chip devices.

#![forbid(unsafe_code)]

pub(crate) mod a27l2;
mod control;
pub(crate) mod usb_dma_bypass;

use runtime::{
    PlatformView, Result,
    memory::{MemoryRegistry, SupervisorMemory},
    soc::allwinner::v821::AllwinnerV821Soc,
};

use crate::{
    driver::platform::{Board, interrupts::TimerDesc},
    sbi::{pmu::SbiPmu, vendor::Vendor},
};

impl Board for AllwinnerV821Soc {
    fn early_init(
        &self,
        memory: &mut MemoryRegistry,
        timer: Option<&TimerDesc>,
    ) -> Result<Option<u64>> {
        let alias = control::early_init(*self, memory)?;
        if matches!(timer, Some(TimerDesc::Plmt(_))) {
            const PLMT_CLOCK_ENABLE: u32 = 1 << 31;
            let clock = memory.acquire(self.plmt_clock()?)?;
            let value = u32::from_le(clock.read::<u32>(0)?);
            clock.write(0, (value | PLMT_CLOCK_ENABLE).to_le())?;
            runtime::instruction::fence();
        }
        Ok(Some(alias.offset()))
    }

    fn init(
        &self,
        platform: &PlatformView<'_>,
        memory: &mut MemoryRegistry,
        supervisor_memory: &'static SupervisorMemory,
        _pmu: &mut Option<SbiPmu>,
    ) -> Result<Option<Vendor>> {
        Vendor::init(*self, platform, memory, supervisor_memory).map(Some)
    }
}
