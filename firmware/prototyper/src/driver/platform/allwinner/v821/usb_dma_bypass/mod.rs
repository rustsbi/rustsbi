//! V821 USB DMA bypass device.

mod description;
mod device;

use runtime::{Error, PlatformView, Result};

use crate::devicetree::is_compatible;

pub(crate) use description::Desc;
pub(crate) use device::UsbDmaBypass;

const COMPATIBLE: &str = "allwinner,sunxi-udc";
const DMA_BYPASS_OFFSET: usize = 0x5c0;
const DMA_BYPASS_SIZE: usize = 4;

/// Discovers the single enabled USB controller node and its bypass register.
pub(crate) fn describe(platform: &PlatformView<'_>) -> Result<Option<Desc>> {
    let mut description = None;
    platform.try_for_each_enabled_node(|node, _, _| {
        if !is_compatible(node, COMPATIBLE) {
            return Ok(());
        }
        let registers = platform.device_register(node)?.ok_or(Error::InvalidArgs)?;
        let registers = registers.subrange(DMA_BYPASS_OFFSET, DMA_BYPASS_SIZE)?;
        if description.replace(Desc { registers }).is_some() {
            return Err(Error::InvalidArgs);
        }
        Ok(())
    })?;
    Ok(description)
}
