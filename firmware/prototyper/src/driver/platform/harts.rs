//! Parsing hart and timebase information.

use runtime::{Error, PlatformView, Result, hart::HartId, node_is_enabled};

use crate::devicetree::{is_cpu_node, u32_property};
use crate::sbi::features::{install_extensions, parse_extensions};

/// Reads the timebase and installs hart features.
pub(super) fn describe(platform: &PlatformView<'_>) -> Result<Option<u32>> {
    let cpus = platform
        .find_enabled_node("/cpus")
        .ok_or(Error::InvalidArgs)?;
    let timebase_frequency_hz =
        u32_property(cpus, "timebase-frequency").filter(|frequency| *frequency != 0);

    let current_hart = HartId::current()?;
    for node in cpus.children().filter(|node| is_cpu_node(*node)) {
        let hart_id = PlatformView::hart_id(node)?;
        if !node_is_enabled(node) {
            continue;
        }
        let hart = HartId::from_raw(hart_id).map_err(|_| Error::InvalidArgs)?;
        install_extensions(hart, current_hart, parse_extensions(node));
    }

    Ok(timebase_frequency_hz)
}
