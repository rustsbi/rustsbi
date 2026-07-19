//! Enabled-hart facts derived from the owned boot device tree.

#![expect(
    dead_code,
    reason = "ISA facts remain part of the validated per-hart discovery result"
)]

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use dtoolkit::model::DeviceTreeNode;
use dtoolkit::{Node, Property};

use super::config::NUM_HART_MAX;
use super::dt::{PlatformDtb, cell_count, enabled, read_cells};
use super::facts::DiscoverError;

/// Immutable facts for one enabled architectural hart.
pub struct HartInfo {
    /// Sparse physical hart identity; never a storage index.
    pub id: usize,
    /// RISC-V ISA string or joined extension list used for feature discovery.
    pub isa: String,
}

pub(super) fn discover(tree: &PlatformDtb) -> Result<Vec<HartInfo>, DiscoverError> {
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
        harts.push(HartInfo { id, isa: isa(node) });
    }

    if harts.is_empty() {
        Err(DiscoverError::HartCount)
    } else {
        Ok(harts)
    }
}

fn isa(node: &DeviceTreeNode) -> String {
    if let Some(isa) = node
        .property("riscv,isa")
        .and_then(|property| property.as_str().ok())
    {
        return isa.to_string();
    }

    let mut isa = String::new();
    if let Some(property) = node.property("riscv,isa-extensions") {
        for extension in property.as_str_list() {
            if !isa.is_empty() {
                isa.push(',');
            }
            isa.push_str(extension);
        }
    }
    isa
}
