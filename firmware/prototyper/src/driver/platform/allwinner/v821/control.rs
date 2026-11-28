//! V821 boot handshake and platform-specific devices.

#![forbid(unsafe_code)]

use crate::driver::platform::TimerDesc;
use runtime::{
    Error, Result,
    hart::HartId,
    instruction::{ALL, fence},
    memory::{MemoryRegistry, MmioRegion},
    soc::allwinner::v821::NoncacheableAlias,
};

use super::V821Board;

impl V821Board {
    pub(super) fn initialize_boot_devices(
        &mut self,
        memory: &mut MemoryRegistry,
        timer: Option<&TimerDesc>,
    ) -> Result<NoncacheableAlias> {
        let soc = self.boot.hardware()?;
        let plmt_clock = if matches!(timer, Some(TimerDesc::Plmt(_))) {
            Some(memory.acquire(soc.plmt_clock()?)?)
        } else {
            None
        };
        let hart = HartId::current().map_err(|_| Error::InvalidArgs)?;
        if hart.as_usize() != 0 || HartId::count() != 1 {
            return Err(Error::InvalidArgs);
        }
        let isp_sram_release = memory.acquire(soc.boot0_isp_sram_release()?)?;
        let alias = self.boot.initialize_noncacheable_alias()?;
        release_boot0_isp_sram(&isp_sram_release)?;
        if let Some(clock) = plmt_clock {
            const PLMT_CLOCK_ENABLE: u32 = 1 << 31;
            let value = u32::from_le(clock.read::<u32>(0)?);
            clock.write(0, (value | PLMT_CLOCK_ENABLE).to_le())?;
            fence(ALL, ALL);
        }
        Ok(alias)
    }
}

/// Lets the RTOS ISP driver reclaim boot0's SRAM after firmware moves to DRAM.
fn release_boot0_isp_sram(registers: &MmioRegion) -> Result<()> {
    const BOOT0_ISP_SRAM_RELEASED: u32 = 1 << 0;

    fence(ALL, ALL);
    let flags = registers.read::<u32>(0)?;
    registers.write(0, flags | BOOT0_ISP_SRAM_RELEASED)?;
    fence(ALL, ALL);
    Ok(())
}
