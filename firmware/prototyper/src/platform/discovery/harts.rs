//! Hart and model discovery.

use alloc::string::ToString;

use runtime::node_is_enabled;

use crate::devicetree::is_cpu_node;
use crate::devicetree::u32_property;
use crate::platform::info::BoardInfo;
use crate::sbi::features::detect_extensions;

pub(super) fn discover(
    board: &mut BoardInfo,
    platform: &runtime::PlatformView<'_>,
) -> runtime::Result<()> {
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

    for node in cpus.children().filter(|node| is_cpu_node(*node)) {
        if !node_is_enabled(node) {
            continue;
        }
        let hart_id = node
            .reg()
            .and_then(|mut registers| registers.next())
            .map(|register| register.starting_address as usize)
            .ok_or(runtime::Error::InvalidArgs)?;
        let enabled = board
            .harts
            .enabled
            .get_mut(hart_id)
            .ok_or(runtime::Error::InvalidArgs)?;
        *enabled = true;
        board.harts.count += 1;
    }

    // TODO: Move ISA-extension discovery behind the Runtime seam too.
    detect_extensions(cpus, &board.harts.enabled);
    Ok(())
}
