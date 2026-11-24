//! Reset-description selection from the device tree.

use runtime::{PlatformView, Result};

use crate::devicetree::{EnabledNode, try_for_each_enabled_node};

use super::super::select_once;
use super::{pmic_spacemit_p1, sifive_test, sunxi_watchdog, syscon};

/// A selected reset description whose MMIO windows have not been acquired.
pub(crate) enum Desc {
    SifiveTest(sifive_test::Desc),
    SpacemitP1(pmic_spacemit_p1::Desc),
    SunxiV105(sunxi_watchdog::v105::Desc),
    SunxiV104(sunxi_watchdog::v104::Desc),
    Syscon(syscon::Desc),
}

/// Selects the highest-priority reset description without acquiring MMIO.
///
/// An error in a higher-priority candidate stops discovery before fallback.
pub(crate) fn describe(
    platform: &PlatformView<'_>,
    timebase_frequency_hz: Option<u32>,
) -> Result<Option<Desc>> {
    if let Some(desc) = find(platform, |node| sifive_test::describe(platform, node))? {
        return Ok(Some(Desc::SifiveTest(desc)));
    }
    if let Some(desc) = find(platform, |node| {
        pmic_spacemit_p1::describe(platform, node, timebase_frequency_hz)
    })? {
        return Ok(Some(Desc::SpacemitP1(desc)));
    }
    if let Some(desc) = sunxi_watchdog::v105::describe(platform)? {
        return Ok(Some(Desc::SunxiV105(desc)));
    }
    if let Some(desc) = find(platform, |node| {
        sunxi_watchdog::v104::describe(platform, node, timebase_frequency_hz)
    })? {
        return Ok(Some(Desc::SunxiV104(desc)));
    }
    Ok(syscon::describe(platform)?.map(Desc::Syscon))
}

/// Finds a unique description among enabled nodes.
///
/// Returns [`runtime::Error::InvalidArgs`] on a second match.
pub(super) fn find<'view, 'tree: 'view, T>(
    platform: &'view PlatformView<'tree>,
    mut describe: impl FnMut(EnabledNode<'view, 'tree>) -> Result<Option<T>>,
) -> Result<Option<T>> {
    let mut selected = None;
    try_for_each_enabled_node(platform, |node, _| {
        if let Some(description) = describe(node)? {
            select_once(&mut selected, description)?;
        }
        Ok(())
    })?;
    Ok(selected)
}
