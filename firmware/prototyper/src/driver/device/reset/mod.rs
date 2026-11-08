//! System-reset firmware drivers.
//!
//! Discovery selects one supported reset description without acquiring MMIO.
//! Initialization then creates the single [`ResetDevice`] managed by `runtime`.

mod pmic_spacemit_p1;
mod selection;
mod sifive_test;
mod sunxi_watchdog;
mod syscon;

use runtime::{Error, FdtNode, PlatformView, Result, memory::RegRange};

pub(crate) use runtime::reset::{ResetDevice, ResetReason, ResetRequest, ResetType};
pub(crate) use selection::describe;

/// Stores `value` in `slot` and rejects a second match.
///
/// Reset discovery treats duplicate matches as an invalid platform description
/// rather than silently choosing the first one.
fn set_once<T>(slot: &mut Option<T>, value: T) -> Result<()> {
    if slot.replace(value).is_some() {
        return Err(Error::InvalidArgs);
    }
    Ok(())
}

fn primary_registers(platform: &PlatformView<'_>, node: FdtNode<'_, '_>) -> Result<RegRange> {
    platform.device_register(node)?.ok_or(Error::InvalidArgs)
}
