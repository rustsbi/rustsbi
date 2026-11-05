//! Reset-description selection from the device tree.

use alloc::boxed::Box;
use runtime::{PlatformView, Result};

use crate::{
    devicetree::{EnabledNode, try_for_each_enabled_node},
    driver::{Description, match_desc},
};

use super::{ResetDevice, pmic_spacemit_p1, set_once, sifive_test, sunxi_watchdog, syscon};

/// A boxed reset description selected for initialization.
pub(crate) type Desc = Box<dyn Description<Device = Box<dyn ResetDevice>>>;

/// Selects the highest-priority reset description without acquiring MMIO.
///
/// Candidate order is priority order, and the first description wins.
pub(crate) fn describe(
    platform: &PlatformView<'_>,
    timebase_frequency_hz: Option<u32>,
) -> Result<Option<Desc>> {
    match_desc!(priority Desc;
        find(platform, |node| sifive_test::describe(platform, node)),
        find(platform, |node| pmic_spacemit_p1::describe(platform, node, timebase_frequency_hz)),
        sunxi_watchdog::v105::describe(platform),
        find(platform, |node| sunxi_watchdog::v104::describe(platform, node, timebase_frequency_hz)),
        syscon::describe(platform),
    )
}

/// Finds the unique reset description of type `T` among enabled nodes.
///
/// A second match is rejected with [`runtime::Error::InvalidArgs`]
/// instead of replacing the first.
pub(super) fn find<'view, 'tree: 'view, T>(
    platform: &'view PlatformView<'tree>,
    mut describe: impl FnMut(EnabledNode<'view, 'tree>) -> Result<Option<T>>,
) -> Result<Option<T>> {
    let mut selected = None;
    try_for_each_enabled_node(platform.root(), &mut |node, _| {
        if let Some(description) = describe(node)? {
            set_once(&mut selected, description)?;
        }
        Ok(())
    })?;
    Ok(selected)
}
