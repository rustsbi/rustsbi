//! Hart and model discovery.

use alloc::string::ToString;

use runtime::node_is_enabled;
use serde_device_tree::buildin::Node;

use crate::devicetree::{Cpu, Tree};
use crate::platform::info::BoardInfo;
use crate::sbi::features::detect_extensions;

pub(super) fn discover(board: &mut BoardInfo, tree: &Tree<'_>) -> runtime::Result<()> {
    board.timebase_frequency_hz = tree
        .cpus
        .timebase_frequency_hz
        .filter(|frequency| *frequency != 0);
    board.model = tree
        .model
        .as_ref()
        .and_then(|model| model.iter().next())
        .unwrap_or("<unspecified>")
        .to_string();

    for cpu_node in tree.cpus.cpu.iter() {
        let node = cpu_node.deserialize::<Node>();
        if !node_is_enabled(&node) {
            continue;
        }
        let cpu = cpu_node.deserialize::<Cpu>();
        let hart_id = cpu
            .reg
            .iter()
            .next()
            .map(|register| register.0.start)
            .ok_or(runtime::Error::InvalidArgs)?;
        let enabled = board
            .enabled_harts
            .get_mut(hart_id)
            .ok_or(runtime::Error::InvalidArgs)?;
        *enabled = true;
        board.hart_count += 1;
    }

    // TODO: Move ISA-extension discovery behind the Runtime seam too.
    detect_extensions(&tree.cpus.cpu, &board.enabled_harts);
    Ok(())
}
