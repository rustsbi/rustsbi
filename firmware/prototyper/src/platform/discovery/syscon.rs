//! Resolve syscon reset descriptions without touching MMIO.
//!
//! Supports an `offset` within either an explicit `regmap` phandle or the
//! parent syscon. A legacy `mask` without `value` is a full-word write of that
//! mask, as in OpenSBI's `fdt_reset_syscon.c`.

use core::mem::{align_of, size_of};
use runtime::{Error, Result, node_is_enabled};
use serde_device_tree::buildin::{Node, StrSeq};

use crate::driver::SysconConfig;
use crate::platform::info::BoardInfo;

pub(super) fn discover_poweroff<'tree>(
    board: &mut BoardInfo,
    platform: &runtime::PlatformView<'tree>,
    node: &Node<'tree>,
    parent: Option<&Node<'tree>>,
) -> Result<()> {
    select(
        &mut board.syscon_poweroff,
        discover(platform, node, parent)?,
    );
    Ok(())
}

pub(super) fn discover_reboot<'tree>(
    board: &mut BoardInfo,
    platform: &runtime::PlatformView<'tree>,
    node: &Node<'tree>,
    parent: Option<&Node<'tree>>,
) -> Result<()> {
    select(&mut board.syscon_reboot, discover(platform, node, parent)?);
    Ok(())
}

fn select(selected: &mut Option<SysconConfig>, config: SysconConfig) {
    // Prefer the highest priority for this peripheral; keep the first tie.
    if selected.is_none_or(|old| config.priority > old.priority) {
        *selected = Some(config);
    }
}

fn discover<'tree>(
    platform: &runtime::PlatformView<'tree>,
    node: &Node<'tree>,
    parent: Option<&Node<'tree>>,
) -> Result<SysconConfig> {
    let provider = resolve_provider(platform.root(), node, parent)?;
    validate_provider(&provider)?;
    let offset = read_u32(node, "offset")?.ok_or(Error::InvalidArgs)?;
    let offset = usize::try_from(offset).map_err(|_| Error::Overflow)?;
    if !offset.is_multiple_of(size_of::<u32>()) {
        return Err(Error::InvalidArgs);
    }
    let (value, mask) = value_and_mask(read_u32(node, "value")?, read_u32(node, "mask")?)?;
    let registers = platform
        .device_registers(&provider)?
        .and_then(|ranges| ranges.first().copied())
        .ok_or(Error::InvalidArgs)?
        .subrange(offset, size_of::<u32>())?;
    if !registers.start().is_aligned_to(align_of::<u32>()) {
        return Err(Error::InvalidArgs);
    }
    Ok(SysconConfig {
        registers,
        value,
        mask,
        priority: read_u32(node, "priority")?.unwrap_or(192),
    })
}

fn value_and_mask(value: Option<u32>, mask: Option<u32>) -> Result<(u32, u32)> {
    match (value, mask) {
        (Some(value), Some(mask)) => Ok((value, mask)),
        (Some(value), None) | (None, Some(value)) => Ok((value, u32::MAX)),
        (None, None) => Err(Error::InvalidArgs),
    }
}

fn read_u32(node: &Node<'_>, name: &str) -> Result<Option<u32>> {
    node.get_prop(name)
        .map(|property| {
            let bytes = property.deserialize::<&[u8]>();
            let bytes = bytes.try_into().map_err(|_| Error::InvalidArgs)?;
            Ok(u32::from_be_bytes(bytes))
        })
        .transpose()
}

fn resolve_provider<'tree>(
    root: &Node<'tree>,
    node: &Node<'tree>,
    parent: Option<&Node<'tree>>,
) -> Result<Node<'tree>> {
    if let Some(phandle) = read_u32(node, "regmap")? {
        if phandle == 0 || phandle == u32::MAX {
            return Err(Error::InvalidArgs);
        }
        find_phandle(root, phandle).ok_or(Error::InvalidArgs)
    } else {
        parent.cloned().ok_or(Error::InvalidArgs)
    }
}

fn find_phandle<'tree>(node: &Node<'tree>, phandle: u32) -> Option<Node<'tree>> {
    // A disabled ancestor also makes a referenced provider unavailable.
    if !node_is_enabled(node) {
        return None;
    }
    let handle = read_u32(node, "phandle")
        .ok()
        .flatten()
        .or_else(|| read_u32(node, "linux,phandle").ok().flatten());
    if handle == Some(phandle) {
        return Some(node.clone());
    }
    node.nodes()
        .find_map(|child| find_phandle(&child.deserialize::<Node<'tree>>(), phandle))
}

fn validate_provider(node: &Node<'_>) -> Result<()> {
    let is_syscon = node.get_prop("compatible").is_some_and(|property| {
        property
            .deserialize::<StrSeq>()
            .iter()
            .any(|value| value == "syscon")
    });
    if !node_is_enabled(node)
        || !is_syscon
        || read_u32(node, "reg-io-width")?.is_some_and(|width| width != 4)
        || node.get_prop("big-endian").is_some()
    {
        return Err(Error::InvalidArgs);
    }
    Ok(())
}
