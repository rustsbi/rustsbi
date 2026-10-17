//! Hart and model discovery.

use alloc::{string::ToString, vec::Vec};
use runtime::{Error, PlatformView, Result, hart::HartId};

use crate::devicetree;
use crate::platform::info::BoardInfo;

use super::imsic::{self, CpuInterruptController};

/// Reads hart topology and features, retaining CPU interrupt wiring for IMSIC discovery.
pub(super) fn discover(
    board: &mut BoardInfo,
    platform: &PlatformView<'_>,
) -> Result<Vec<CpuInterruptController>> {
    let root = platform.root();
    let cpus = platform
        .find_enabled_node("/cpus")
        .ok_or(Error::InvalidArgs)?;
    board.timebase_frequency_hz =
        devicetree::u32_property(cpus, "timebase-frequency").filter(|frequency| *frequency != 0);
    board.model = root
        .property("model")
        .and_then(|property| property.as_str())
        .unwrap_or("<unspecified>")
        .to_string();

    let mut controllers = Vec::new();
    for node in cpus
        .children()
        .filter(|node| devicetree::is_cpu_node(*node))
    {
        let hart_id = PlatformView::hart_id(node)?;
        controllers.extend(
            node.children()
                .filter_map(|child| imsic::cpu_interrupt_controller(child, hart_id)),
        );
        if !runtime::node_is_enabled(node) {
            continue;
        }
        HartId::from_raw(hart_id).map_err(|_| Error::InvalidArgs)?;
    }

    Ok(controllers)
}
