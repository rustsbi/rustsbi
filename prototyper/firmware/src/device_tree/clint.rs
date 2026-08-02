//! Discovery of the selected CLINT node.

use alloc::string::String;

use dtoolkit::model::DeviceTreeNode;
use dtoolkit::{Node, Property};

use super::Error as DiscoverError;
use super::dt::{enabled, nodes_with_paths, reg_at_path};

const SIFIVE_COMPATIBLE: [&str; 3] = ["riscv,clint0", "starfive,jh7110-clint", "sifive,clint0"];
const THEAD_COMPATIBLE: [&str; 1] = ["thead,c900-clint"];

/// Validated CLINT node selected for machine installation.
pub(super) struct Clint {
    pub(super) path: String,
    pub(super) layout: machine::interrupt::clint::Layout,
}

pub(super) fn discover(
    root: &DeviceTreeNode,
    harts: &[usize],
) -> Result<Option<Clint>, DiscoverError> {
    let mut selected = None;
    for (node, path) in nodes_with_paths(root) {
        if !enabled(node) {
            continue;
        }
        let supported = node.property("compatible").is_some_and(|property| {
            property.as_str_list().any(|compatible| {
                SIFIVE_COMPATIBLE.contains(&compatible) || THEAD_COMPATIBLE.contains(&compatible)
            })
        });
        if !supported {
            continue;
        }
        if selected.is_some() {
            return Err(DiscoverError::AmbiguousDevice);
        }
        let range = reg_at_path(root, &path).map_err(|_| DiscoverError::DeviceRange)?;
        let time = node.property("compatible").is_some_and(|property| {
            property
                .as_str_list()
                .any(|compatible| THEAD_COMPATIBLE.contains(&compatible))
        }) || (node.property("compatible").is_some_and(|property| {
            property
                .as_str_list()
                .any(|compatible| SIFIVE_COMPATIBLE.contains(&compatible))
        }) && node.property("clint,has-no-64bit-mmio").is_some());
        let time = if time {
            machine::interrupt::clint::TimeSource::Counter
        } else {
            machine::interrupt::clint::TimeSource::MemoryMapped
        };
        let layout = machine::interrupt::clint::Layout::new(range, harts.to_vec(), time)
            .ok_or(DiscoverError::DeviceRange)?;
        selected = Some(Clint { path, layout });
    }
    Ok(selected)
}
