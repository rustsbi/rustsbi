//! Hart and model discovery.

use alloc::{string::ToString, vec::Vec};
use runtime::{Error, PlatformView, Result, hart::HartId, node_is_enabled};

use crate::devicetree::{is_cpu_node, u32_property};
use crate::platform::info::BoardInfo;
use crate::sbi::features::detect_extensions;

use super::cpu_intc::CpuInterruptController;

/// Reads hart topology and features.
///
/// Interrupt wiring is retained for every CPU node, including disabled CPUs,
/// because devices such as ACLINT keep a physical slot for each wiring entry.
pub(super) fn discover(
    board: &mut BoardInfo,
    platform: &PlatformView<'_>,
) -> Result<Vec<CpuInterruptController>> {
    let root = platform.root();
    let cpus = platform
        .find_enabled_node("/cpus")
        .ok_or(Error::InvalidArgs)?;
    board.timebase_frequency_hz =
        u32_property(cpus, "timebase-frequency").filter(|frequency| *frequency != 0);
    board.model = root
        .property("model")
        .and_then(|property| property.as_str())
        .unwrap_or("<unspecified>")
        .to_string();

    let mut controllers = Vec::new();
    for node in cpus.children().filter(|node| is_cpu_node(*node)) {
        let hart_id = PlatformView::hart_id(node)?;
        let cpu_enabled = node_is_enabled(node);
        controllers.extend(
            node.children()
                .filter_map(|child| CpuInterruptController::from_node(child, hart_id)),
        );
        if !cpu_enabled {
            continue;
        }
        HartId::from_raw(hart_id).map_err(|_| Error::InvalidArgs)?;
        detect_extensions(hart_id, node);
    }

    Ok(controllers)
}
