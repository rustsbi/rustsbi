//! Console discovery from `/chosen/stdout-path` with an FDT-wide fallback.

use crate::driver;
use crate::platform::info::ConsoleInfo;

pub(super) fn discover(
    platform: &runtime::PlatformView<'_>,
) -> runtime::Result<Option<ConsoleInfo>> {
    if let Some(chosen) = platform.find_enabled_node("/chosen")
        && let Some(stdout_path) = chosen
            .property("stdout-path")
            .and_then(|property| property.as_str())
            .and_then(|path| path.split(':').next())
        && let Some(node) = platform.find_enabled_node(stdout_path)
        && let Some(console) = discover_node(platform, node)?
    {
        return Ok(Some(console));
    }

    for node in platform.fdt().all_nodes() {
        if !runtime::node_is_enabled(node) {
            continue;
        }
        if let Some(console) = discover_node(platform, node)? {
            return Ok(Some(console));
        }
    }
    Ok(None)
}

fn discover_node(
    platform: &runtime::PlatformView<'_>,
    node: runtime::FdtNode<'_, '_>,
) -> runtime::Result<Option<ConsoleInfo>> {
    let Some(compatibles) = node.compatible() else {
        return Ok(None);
    };

    let mut register_shift = None;
    let mut register_width = None;
    let mut clock_hz = None;
    for property in node.properties() {
        let slot = match property.name {
            "reg-shift" => &mut register_shift,
            "reg-io-width" => &mut register_width,
            "clock-frequency" => &mut clock_hz,
            _ => continue,
        };
        // Preserve `FdtNode::property`'s first-match behavior for malformed values.
        slot.get_or_insert(property.value);
    }
    let parse_u32 = |value: &[u8]| value.try_into().ok().map(u32::from_be_bytes);
    let register_shift = register_shift.and_then(parse_u32);
    let register_width = register_width.and_then(parse_u32);
    let clock_hz = clock_hz.and_then(parse_u32);
    let Some(kind) = compatibles.all().find_map(|compatible| {
        driver::ConsoleKind::from_fdt(compatible, register_shift, register_width)
    }) else {
        return if compatibles.all().any(driver::ConsoleKind::supports) {
            Err(runtime::Error::InvalidArgs)
        } else {
            Ok(None)
        };
    };
    let registers = platform
        .device_register(node)?
        .ok_or(runtime::Error::InvalidArgs)?;

    Ok(Some(ConsoleInfo {
        registers,
        kind,
        clock_hz,
    }))
}
