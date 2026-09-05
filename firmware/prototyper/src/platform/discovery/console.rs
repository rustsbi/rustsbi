//! Console discovery from `/chosen/stdout-path`.

use crate::devicetree::{compatible_strings, find_enabled_node};
use crate::driver;
use crate::platform::info::ConsoleInfo;

pub(super) fn discover(
    platform: &runtime::PlatformView<'_>,
) -> runtime::Result<Option<ConsoleInfo>> {
    let root = platform.root();
    if find_enabled_node(root, "/chosen").is_none() {
        return Ok(None);
    }
    let Some(stdout_path) = root.chosen_stdout_path() else {
        return Ok(None);
    };
    if root.find(stdout_path).is_none() {
        return Err(runtime::Error::InvalidArgs);
    }
    let Some(node) = find_enabled_node(root, stdout_path) else {
        return Ok(None);
    };
    let compatibles = compatible_strings(&node).ok_or(runtime::Error::InvalidArgs)?;

    let register_shift = node
        .get_prop("reg-shift")
        .map(|property| property.deserialize::<u32>());
    let register_width = node
        .get_prop("reg-io-width")
        .map(|property| property.deserialize::<u32>());
    let kind = compatibles.iter().find_map(|compatible| {
        driver::ConsoleKind::from_fdt(compatible, register_shift, register_width)
    });
    let Some(kind) = kind else {
        return if compatibles.iter().any(driver::ConsoleKind::supports) {
            Err(runtime::Error::InvalidArgs)
        } else {
            Ok(None)
        };
    };
    let registers = platform
        .device_registers(&node)?
        .and_then(|ranges| ranges.first().copied())
        .ok_or(runtime::Error::InvalidArgs)?;

    Ok(Some(ConsoleInfo {
        registers,
        kind,
        clock_hz: node
            .get_prop("clock-frequency")
            .map(|property| property.deserialize::<u32>()),
    }))
}
