//! Platform-specific secondary-hart wakeup selection.

use alloc::boxed::Box;
use runtime::{
    PlatformView, Result, hart::HartWake, memory::MemoryRegistry,
    soc::allwinner::v821::AllwinnerV821Soc,
};

use super::{allwinner::v861, spacemit_k1};
use crate::driver::Description as _;

/// Initializes the selected platform's hart wake device, if one is provided.
pub(super) fn init(
    platform: &PlatformView<'_>,
    memory: &mut MemoryRegistry,
) -> Result<Option<Box<dyn HartWake>>> {
    // Keep the root-platform priority shared by platform preparation and PMU.
    if platform.soc::<AllwinnerV821Soc>()?.is_some() {
        return Ok(None);
    }
    if let Some(description) = spacemit_k1::describe(platform)? {
        return spacemit_k1::init(description, memory).map(Some);
    }
    v861::describe(platform)?
        .map(|description| Box::new(description).init(memory))
        .transpose()
}
