//! V821 platform description and initialization.

#![forbid(unsafe_code)]

pub(crate) mod a27l2;
mod control;
mod description;
mod device;
pub(crate) mod usb_dma_bypass;

use runtime::{
    PlatformView, Result, memory::MemoryRegistry, soc::allwinner::v821::AllwinnerV821Soc,
};

pub(crate) use description::Desc;
pub(crate) use device::V821;

pub(crate) fn describe(platform: &PlatformView<'_>) -> Result<Option<Desc>> {
    let Some(soc) = platform.soc::<AllwinnerV821Soc>()? else {
        return Ok(None);
    };
    Ok(Some(Desc::new(soc)))
}

/// Enables the PLMT clock when the V821 platform is present.
pub(crate) fn prepare_plmt(platform: Option<&V821>, memory: &mut MemoryRegistry) -> Result<()> {
    if let Some(platform) = platform {
        const PLMT_CLOCK_ENABLE: u32 = 1 << 31;
        let clock = memory.acquire(platform.soc().plmt_clock()?)?;
        let value = u32::from_le(clock.read::<u32>(0)?);
        clock.write(0, (value | PLMT_CLOCK_ENABLE).to_le())?;
        runtime::instruction::fence();
    }
    Ok(())
}
