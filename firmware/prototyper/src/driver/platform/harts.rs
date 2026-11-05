//! Parsing hart and timebase information.

use alloc::vec::Vec;
use runtime::{Error, PlatformView, Result, hart::HartId, node_is_enabled};

use crate::devicetree::{is_cpu_node, u32_property};
use crate::driver::device::imsic::{self, CpuInterruptController};
use crate::sbi::features::{install_extensions, parse_extensions};

/// Reads the timebase, installs hart features, and returns CPU interrupt-controller wiring.
pub(super) fn describe(
    platform: &PlatformView<'_>,
) -> Result<(Option<u32>, Vec<CpuInterruptController>)> {
    let cpus = platform
        .find_enabled_node("/cpus")
        .ok_or(Error::InvalidArgs)?;
    let timebase_frequency_hz =
        u32_property(cpus, "timebase-frequency").filter(|frequency| *frequency != 0);

    let mut controllers = Vec::new();
    for node in cpus.children().filter(|node| is_cpu_node(*node)) {
        let hart_id = PlatformView::hart_id(node)?;
        controllers.extend(
            node.children()
                .filter_map(|child| imsic::cpu_interrupt_controller(child, hart_id)),
        );
        if !node_is_enabled(node) {
            continue;
        }
        let hart = HartId::from_raw(hart_id).map_err(|_| Error::InvalidArgs)?;
        install_extensions(hart, parse_extensions(node));
    }

    Ok((timebase_frequency_hz, controllers))
}
