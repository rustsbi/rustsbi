//! Inert facts for the selected CLINT node.

#![expect(
    dead_code,
    reason = "complete validated facts are retained for DT policy and construction cross-checks"
)]

use alloc::string::String;
use core::ops::Range;

use dtoolkit::model::DeviceTreeNode;
use dtoolkit::{Node, Property};

use super::dt::{enabled, nodes_with_paths, reg_at_path};
use super::facts::DiscoverError;

const SIFIVE_COMPATIBLE: [&str; 3] = ["riscv,clint0", "starfive,jh7110-clint", "sifive,clint0"];
const THEAD_COMPATIBLE: [&str; 1] = ["thead,c900-clint"];

/// Inert facts for one selected CLINT node.
pub struct Clint {
    /// Exact node identity retained for supervisor visibility policy and
    /// machine-side binding validation.
    pub path: String,
    /// Checked physical register range; this value grants no MMIO authority.
    pub range: Range<usize>,
    /// Register convention selected from the compatible binding.
    pub kind: ClintKind,
}

/// Supported CLINT register conventions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClintKind {
    /// Standard SiFive-compatible CLINT layout.
    SiFive,
    /// T-Head CLINT layout without atomic 64-bit MMIO.
    THead,
}

pub(super) fn discover(root: &DeviceTreeNode) -> Result<Option<Clint>, DiscoverError> {
    let mut selected = None;
    for (node, path) in nodes_with_paths(root) {
        if !enabled(node) {
            continue;
        }
        let kind = node.property("compatible").and_then(|property| {
            property.as_str_list().find_map(|compatible| {
                if THEAD_COMPATIBLE.contains(&compatible)
                    || (SIFIVE_COMPATIBLE.contains(&compatible)
                        && node.property("clint,has-no-64bit-mmio").is_some())
                {
                    Some(ClintKind::THead)
                } else if SIFIVE_COMPATIBLE.contains(&compatible) {
                    Some(ClintKind::SiFive)
                } else {
                    None
                }
            })
        });
        let Some(kind) = kind else {
            continue;
        };
        if selected.is_some() {
            return Err(DiscoverError::AmbiguousDevice);
        }
        selected = Some(Clint {
            range: reg_at_path(root, &path).map_err(|_| DiscoverError::DeviceRange)?,
            path,
            kind,
        });
    }
    Ok(selected)
}
