//! Enabled architectural harts derived from the owned boot device tree.

use alloc::vec::Vec;

use dtoolkit::{Node, Property};

use super::config::NUM_HART_MAX;
use super::discovery::Error as DiscoverError;
use super::dt::{BootTree, cell_count, enabled, read_cells};

/// One enabled architectural hart selected during boot.
pub(crate) struct HartInfo {
    /// Sparse physical hart identity; never a storage index.
    pub(super) id: usize,
}

pub(crate) fn discover(tree: &BootTree) -> Result<Vec<HartInfo>, DiscoverError> {
    let cpus = tree
        .tree()
        .root
        .child("cpus")
        .ok_or(DiscoverError::HartCount)?;
    let address_cells = cell_count(cpus, "#address-cells", 1)?;
    let mut harts = Vec::new();

    for node in cpus.children() {
        if node.name_without_address() != "cpu" || !enabled(node) {
            continue;
        }
        if harts.len() == NUM_HART_MAX {
            return Err(DiscoverError::HartCount);
        }
        let reg = node.property("reg").ok_or(DiscoverError::HartId)?;
        let id = read_cells(reg.value(), address_cells).ok_or(DiscoverError::HartId)?;
        let id = usize::try_from(id).map_err(|_| DiscoverError::HartId)?;
        if harts.iter().any(|hart: &HartInfo| hart.id == id) {
            return Err(DiscoverError::DuplicateHart);
        }
        harts.push(HartInfo { id });
    }

    if harts.is_empty() {
        Err(DiscoverError::HartCount)
    } else {
        Ok(harts)
    }
}
