//! V821 A27L2 cache.

mod device;

use runtime::{Error, PlatformView, Result, memory::RegRange};

use crate::devicetree::{is_compatible, u32_property};

pub(crate) use device::A27L2Cache;

const COMPATIBLE: &str = "cache";
const CACHE_LEVEL: u32 = 2;

/// Discovers the single enabled A27L2 cache node.
pub(crate) fn describe(platform: &PlatformView<'_>) -> Result<Option<RegRange>> {
    let mut description = None;
    platform.try_for_each_enabled_node(|node, _, _| {
        if !is_compatible(node, COMPATIBLE)
            || u32_property(node, "cache-level") != Some(CACHE_LEVEL)
        {
            return Ok(());
        }
        let registers = platform.device_register(node)?.ok_or(Error::InvalidArgs)?;
        if description.replace(registers).is_some() {
            return Err(Error::InvalidArgs);
        }
        Ok(())
    })?;
    Ok(description)
}
