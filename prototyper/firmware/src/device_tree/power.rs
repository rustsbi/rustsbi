//! Inert power-control facts derived during cold boot.

use alloc::string::String;
use alloc::vec::Vec;
use core::ops::Range;

use dtoolkit::model::DeviceTreeNode;
use dtoolkit::{Node, Property};

use super::Error as DiscoverError;
use super::dt::{enabled, nodes_with_paths, reg_at_path};

const SIFIVE_TEST_COMPATIBLE: &str = "sifive,test0";

/// Facts for one selected whole-machine power control.
pub struct Power {
    /// Exact DT node identity retained for machine-side binding validation.
    pub path: String,
    /// Checked physical register range; this value grants no MMIO authority.
    pub range: Range<usize>,
    /// Enabled power/reboot consumers that reference the selected register.
    pub consumers: Vec<String>,
}

pub(super) fn discover(root: &DeviceTreeNode) -> Result<Option<Power>, DiscoverError> {
    let mut selected = None;
    for (node, path) in nodes_with_paths(root) {
        if !enabled(node)
            || !node.property("compatible").is_some_and(|property| {
                property
                    .as_str_list()
                    .any(|value| value == SIFIVE_TEST_COMPATIBLE)
            })
        {
            continue;
        }
        if selected.is_some() {
            return Err(DiscoverError::AmbiguousDevice);
        }
        selected = Some((
            Power {
                range: reg_at_path(root, &path).map_err(|_| DiscoverError::DeviceRange)?,
                path,
                consumers: Vec::new(),
            },
            node.property("phandle").and_then(property_u32),
        ));
    }
    let Some((mut power, phandle)) = selected else {
        return Ok(None);
    };
    if let Some(phandle) = phandle {
        for (node, path) in nodes_with_paths(root) {
            let is_consumer = enabled(node)
                && node.property("compatible").is_some_and(|property| {
                    property
                        .as_str_list()
                        .any(|value| value == "syscon-poweroff" || value == "syscon-reboot")
                });
            if !is_consumer {
                continue;
            }
            let regmap = node
                .property("regmap")
                .and_then(property_u32)
                .ok_or(DiscoverError::DeviceRange)?;
            if regmap == phandle {
                power.consumers.push(path);
            }
        }
    }
    Ok(Some(power))
}

pub(crate) fn install(
    boot: &mut machine::BootInfo,
    tree: &mut super::dt::BootTree,
) -> Result<bool, DiscoverError> {
    let Some(power) = discover(&tree.tree().root)? else {
        return Ok(false);
    };
    let io =
        machine::IoMem::acquire(boot, power.range.clone()).ok_or(DiscoverError::Installation)?;
    if !sifive_test::bind(io) {
        return Err(DiscoverError::Installation);
    }
    for consumer in &power.consumers {
        tree.disable_node(consumer)?;
    }
    tree.disable_node(&power.path)?;
    Ok(true)
}

fn property_u32(property: &dtoolkit::model::DeviceTreeProperty) -> Option<u32> {
    let bytes: [u8; 4] = property.value().try_into().ok()?;
    Some(u32::from_be_bytes(bytes))
}
