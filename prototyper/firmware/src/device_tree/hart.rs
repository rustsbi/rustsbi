//! Enabled architectural harts derived from the owned boot device tree.

use alloc::vec::Vec;

use dtoolkit::{Node, Property};

use super::Error as DiscoverError;
use super::dt::{BootTree, cell_count, enabled, read_cells};

const MAX_HARTS: usize = 8;

/// Returns the enabled sparse architectural hart identities for machine setup.
pub(crate) fn discover(tree: &BootTree) -> Result<Vec<usize>, DiscoverError> {
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
        if harts.len() == MAX_HARTS {
            return Err(DiscoverError::HartCount);
        }
        let reg = node.property("reg").ok_or(DiscoverError::HartId)?;
        let id = read_cells(reg.value(), address_cells).ok_or(DiscoverError::HartId)?;
        let id = usize::try_from(id).map_err(|_| DiscoverError::HartId)?;
        if harts.contains(&id) {
            return Err(DiscoverError::DuplicateHart);
        }
        harts.push(id);
    }

    if harts.is_empty() {
        Err(DiscoverError::HartCount)
    } else {
        Ok(harts)
    }
}
