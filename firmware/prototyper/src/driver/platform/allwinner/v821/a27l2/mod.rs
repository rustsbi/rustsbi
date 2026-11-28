//! V821 A27L2 cache.

mod description;
mod device;

use runtime::{Error, PlatformView, Result, soc::allwinner::v821::V821Csr};

use crate::devicetree::{try_for_each_enabled_node, u32_property};

pub(crate) use description::Desc;
pub(crate) use device::A27L2Cache;

const COMPATIBLE: &str = "cache";
const CACHE_LEVEL: u32 = 2;

/// Discovers the single enabled A27L2 cache node.
pub(crate) fn describe(platform: &PlatformView<'_>, csr: V821Csr) -> Result<Option<Desc>> {
    let mut description = None;
    try_for_each_enabled_node(platform, |discovered, _| {
        let Some(compatibles) = discovered.compatible() else {
            return Ok(());
        };
        let node = discovered.node();
        if !compatibles.all().any(|value| value == COMPATIBLE)
            || u32_property(node, "cache-level") != Some(CACHE_LEVEL)
        {
            return Ok(());
        }
        let registers = platform.device_register(node)?.ok_or(Error::InvalidArgs)?;
        if description.replace(Desc { csr, registers }).is_some() {
            return Err(Error::InvalidArgs);
        }
        Ok(())
    })?;
    Ok(description)
}
