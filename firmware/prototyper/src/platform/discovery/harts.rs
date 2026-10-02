//! Hart and model discovery.

use alloc::{string::ToString, vec::Vec};

use runtime::node_is_enabled;

use crate::devicetree::{is_cpu_node, u32_property};
use crate::platform::info::BoardInfo;
use crate::sbi::features::detect_extensions;

use super::imsic::{self, CpuInterruptController};

/// Reads present hart IDs and enabled-hart features and interrupt wiring.
pub(super) fn discover(
    board: &mut BoardInfo,
    platform: &runtime::PlatformView<'_>,
) -> runtime::Result<(Vec<usize>, Vec<CpuInterruptController>)> {
    let root = platform.root();
    let cpus = platform
        .find_enabled_node("/cpus")
        .ok_or(runtime::Error::InvalidArgs)?;
    board.harts.timebase_frequency_hz =
        u32_property(cpus, "timebase-frequency").filter(|frequency| *frequency != 0);
    board.model = root
        .property("model")
        .and_then(|property| property.as_str())
        .unwrap_or("<unspecified>")
        .to_string();

    let mut present = Vec::new();
    let mut controllers = Vec::new();
    for node in cpus.children().filter(|node| is_cpu_node(*node)) {
        let hart_id = node
            .reg()
            .and_then(|mut registers| registers.next())
            .map(|register| register.starting_address as usize);
        if let Some(hart_id) = hart_id {
            // Disabled harts may already be running the firmware and need
            // Runtime state to initialize traps and enter the parked loop.
            present.push(hart_id);
        }
        if !node_is_enabled(node) {
            continue;
        }
        let hart_id = hart_id.ok_or(runtime::Error::InvalidArgs)?;
        let enabled = board
            .harts
            .enabled
            .get_mut(hart_id)
            .ok_or(runtime::Error::InvalidArgs)?;
        *enabled = true;
        board.harts.count += 1;
        detect_extensions(hart_id, node);
        controllers.extend(
            node.children()
                .filter_map(|child| imsic::cpu_interrupt_controller(child, hart_id)),
        );
    }

    Ok((present, controllers))
}
