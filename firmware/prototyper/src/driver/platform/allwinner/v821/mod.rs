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
    match platform {
        Some(platform) => control::enable_plmt_clock(platform.soc(), memory),
        None => Ok(()),
    }
}
