//! Discovery of the retained machine-level IMSIC and APLIC path.

use alloc::string::String;
use alloc::vec::Vec;
use core::ops::Range;

use dtoolkit::model::DeviceTreeNode;
use dtoolkit::{Node, Property};

use super::discovery::Error as DiscoverError;
use super::dt::{BootTree, cell_count, enabled, read_cells};
use super::hart::HartInfo;

const MACHINE_EXTERNAL_INTERRUPT: u32 = 11;
const FIRMWARE_IPI_IID: u16 = 1;
const IMSIC_COMPATIBLE: [&str; 2] = ["riscv,imsics", "riscv,imsic"];
const APLIC_COMPATIBLE: &str = "riscv,aplic";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct HartFile {
    hart_id: usize,
    address: usize,
}

/// Validated IMSIC node selected for machine installation.
pub(super) struct Imsic {
    pub(super) path: String,
}

/// Validated APLIC node selected for machine installation.
pub(super) struct Aplic {
    pub(super) path: String,
}

pub(super) fn discover(
    tree: &BootTree,
    harts: &[HartInfo],
) -> Result<(Option<Imsic>, Option<Aplic>), DiscoverError> {
    let intc_harts = cpu_interrupt_controllers(tree)?;
    let mut imsic = None;
    let mut aplic = None;
    let mut pending = root_children(&tree.tree().root);

    while let Some((node, parent, path)) = pending.pop() {
        for child in node.children() {
            let child_path = join_path(&path, child.name());
            pending.push((child, node, child_path));
        }
        if !enabled(node) {
            continue;
        }

        if compatible(node, &IMSIC_COMPATIBLE) {
            let Some(candidate) = discover_imsic(node, parent, path, &intc_harts, harts)? else {
                continue;
            };
            if imsic.replace(candidate).is_some() {
                return Err(DiscoverError::AmbiguousDevice);
            }
        } else if compatible(node, &[APLIC_COMPATIBLE]) && node.property("riscv,children").is_some()
        {
            let candidate = discover_aplic(node, parent, path)?;
            if aplic.replace(candidate).is_some() {
                return Err(DiscoverError::AmbiguousDevice);
            }
        }
    }

    if imsic.is_some() != aplic.is_some() {
        return Err(DiscoverError::UnsupportedDevice);
    }
    Ok((imsic, aplic))
}

fn discover_imsic(
    node: &DeviceTreeNode,
    parent: &DeviceTreeNode,
    path: String,
    intc_harts: &[(u32, usize)],
    admitted_harts: &[HartInfo],
) -> Result<Option<Imsic>, DiscoverError> {
    let interrupt_cells = u32_cells(
        node.property("interrupts-extended")
            .ok_or(DiscoverError::UnsupportedDevice)?,
    )?;
    let mut chunks = interrupt_cells.chunks_exact(2);
    let mut raw_files = Vec::new();
    for (file_index, entry) in chunks.by_ref().enumerate() {
        if entry[1] != MACHINE_EXTERNAL_INTERRUPT {
            continue;
        }
        let hart_id = intc_harts
            .iter()
            .find_map(|(phandle, hart_id)| (*phandle == entry[0]).then_some(*hart_id))
            .ok_or(DiscoverError::HartId)?;
        let file_index = u32::try_from(file_index).map_err(|_| DiscoverError::DeviceRange)?;
        raw_files.push((hart_id, file_index));
    }
    if !chunks.remainder().is_empty() {
        return Err(DiscoverError::UnsupportedDevice);
    }
    if raw_files.is_empty() {
        return Ok(None);
    }
    for hart in admitted_harts {
        if !raw_files.iter().any(|(hart_id, _)| *hart_id == hart.id) {
            return Err(DiscoverError::UnsupportedDevice);
        }
    }

    let ranges = reg_ranges(parent, node)?;
    let num_ids = required_u32(node, "riscv,num-ids")?;
    let num_ids = u16::try_from(num_ids).map_err(|_| DiscoverError::DeviceRange)?;
    if num_ids <= FIRMWARE_IPI_IID {
        return Err(DiscoverError::UnsupportedDevice);
    }
    let default_hart_bits = topology_bits(raw_files.len())?;
    let hart_index_bits = optional_u32(node, "riscv,hart-index-bits", default_hart_bits)?;
    let group_index_bits = optional_u32(node, "riscv,group-index-bits", 0)?;
    let group_index_shift = optional_u32(node, "riscv,group-index-shift", 24)?;
    validate_topology(hart_index_bits, group_index_bits, group_index_shift)?;

    let base = ranges.first().ok_or(DiscoverError::DeviceRange)?.start;
    let mut hart_files = Vec::new();
    for (hart_id, file_index) in raw_files {
        let address = file_address(
            base,
            file_index,
            hart_index_bits,
            group_index_bits,
            group_index_shift,
        )?;
        let end = address
            .checked_add(0x1000)
            .ok_or(DiscoverError::DeviceRange)?;
        if !ranges
            .iter()
            .any(|range| address >= range.start && end <= range.end)
        {
            return Err(DiscoverError::DeviceRange);
        }
        if hart_files
            .iter()
            .any(|file: &HartFile| file.hart_id == hart_id || file.address == address)
        {
            return Err(DiscoverError::UnsupportedDevice);
        }
        hart_files.push(HartFile { hart_id, address });
    }

    Ok(Some(Imsic { path }))
}

fn discover_aplic(
    node: &DeviceTreeNode,
    parent: &DeviceTreeNode,
    path: String,
) -> Result<Aplic, DiscoverError> {
    let ranges = reg_ranges(parent, node)?;
    let mut ranges = ranges.into_iter();
    ranges.next().ok_or(DiscoverError::DeviceRange)?;
    if ranges.next().is_some() {
        return Err(DiscoverError::DeviceRange);
    }
    if required_u32(node, "riscv,num-sources")? == 0 {
        return Err(DiscoverError::UnsupportedDevice);
    }
    Ok(Aplic { path })
}

fn cpu_interrupt_controllers(tree: &BootTree) -> Result<Vec<(u32, usize)>, DiscoverError> {
    let cpus = tree
        .tree()
        .root
        .child("cpus")
        .ok_or(DiscoverError::HartCount)?;
    let address_cells = cell_count(cpus, "#address-cells", 1)?;
    let mut result = Vec::new();
    for cpu in cpus.children() {
        if cpu.name_without_address() != "cpu" || !enabled(cpu) {
            continue;
        }
        let hart_id = read_cells(
            cpu.property("reg").ok_or(DiscoverError::HartId)?.value(),
            address_cells,
        )
        .ok_or(DiscoverError::HartId)?;
        let hart_id = usize::try_from(hart_id).map_err(|_| DiscoverError::HartId)?;
        for intc in cpu.children() {
            if !compatible(intc, &["riscv,cpu-intc"]) {
                continue;
            }
            let phandle = intc
                .property("phandle")
                .or_else(|| intc.property("linux,phandle"))
                .ok_or(DiscoverError::HartId)?
                .as_u32()
                .map_err(|_| DiscoverError::HartId)?;
            if result.iter().any(|(known, _)| *known == phandle) {
                return Err(DiscoverError::HartId);
            }
            result.push((phandle, hart_id));
        }
    }
    Ok(result)
}

fn root_children(root: &DeviceTreeNode) -> Vec<(&DeviceTreeNode, &DeviceTreeNode, String)> {
    root.children()
        .map(|child| (child, root, join_path("", child.name())))
        .collect()
}

fn join_path(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        alloc::format!("/{name}")
    } else {
        alloc::format!("{parent}/{name}")
    }
}

fn compatible(node: &DeviceTreeNode, accepted: &[&str]) -> bool {
    node.property("compatible").is_some_and(|property| {
        property
            .as_str_list()
            .any(|value| accepted.contains(&value))
    })
}

fn reg_ranges(
    parent: &DeviceTreeNode,
    node: &DeviceTreeNode,
) -> Result<Vec<Range<usize>>, DiscoverError> {
    if parent
        .property("ranges")
        .is_some_and(|property| !property.value().is_empty())
    {
        return Err(DiscoverError::UnsupportedDevice);
    }
    let address_cells = cell_count(parent, "#address-cells", 2)?;
    let size_cells = cell_count(parent, "#size-cells", 1)?;
    let entry_cells = address_cells
        .checked_add(size_cells)
        .ok_or(DiscoverError::DeviceRange)?;
    let cells = u32_cells(node.property("reg").ok_or(DiscoverError::DeviceRange)?)?;
    let mut chunks = cells.chunks_exact(entry_cells);
    let mut result = Vec::new();
    for entry in chunks.by_ref() {
        let start = read_u32_cells(&entry[..address_cells])?;
        let size = read_u32_cells(&entry[address_cells..])?;
        let start = usize::try_from(start).map_err(|_| DiscoverError::DeviceRange)?;
        let size = usize::try_from(size).map_err(|_| DiscoverError::DeviceRange)?;
        let end = start.checked_add(size).ok_or(DiscoverError::DeviceRange)?;
        if start == end || start & 0xfff != 0 || size & 0xfff != 0 {
            return Err(DiscoverError::DeviceRange);
        }
        result.push(start..end);
    }
    if !chunks.remainder().is_empty() || result.is_empty() {
        return Err(DiscoverError::DeviceRange);
    }
    Ok(result)
}

fn u32_cells(property: &dtoolkit::model::DeviceTreeProperty) -> Result<Vec<u32>, DiscoverError> {
    let mut chunks = property.value().chunks_exact(4);
    let cells = chunks
        .by_ref()
        .map(|chunk| u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();
    if chunks.remainder().is_empty() {
        Ok(cells)
    } else {
        Err(DiscoverError::DeviceRange)
    }
}

fn read_u32_cells(cells: &[u32]) -> Result<u64, DiscoverError> {
    if cells.len() > 2 {
        return Err(DiscoverError::DeviceRange);
    }
    cells.iter().try_fold(0u64, |value, cell| {
        value
            .checked_shl(32)
            .map(|value| value | u64::from(*cell))
            .ok_or(DiscoverError::DeviceRange)
    })
}

fn required_u32(node: &DeviceTreeNode, name: &str) -> Result<u32, DiscoverError> {
    node.property(name)
        .ok_or(DiscoverError::UnsupportedDevice)?
        .as_u32()
        .map_err(|_| DiscoverError::UnsupportedDevice)
}

fn optional_u32(node: &DeviceTreeNode, name: &str, default: u32) -> Result<u32, DiscoverError> {
    node.property(name)
        .map(|property| {
            property
                .as_u32()
                .map_err(|_| DiscoverError::UnsupportedDevice)
        })
        .unwrap_or(Ok(default))
}

fn topology_bits(count: usize) -> Result<u32, DiscoverError> {
    let count = u32::try_from(count).map_err(|_| DiscoverError::DeviceRange)?;
    Ok(if count <= 1 {
        0
    } else {
        u32::BITS - (count - 1).leading_zeros()
    })
}

fn validate_topology(
    hart_bits: u32,
    group_bits: u32,
    group_shift: u32,
) -> Result<(), DiscoverError> {
    if hart_bits >= usize::BITS
        || group_bits >= usize::BITS
        || hart_bits
            .checked_add(group_bits)
            .is_none_or(|bits| bits >= usize::BITS)
        || group_shift < 12u32.saturating_add(hart_bits)
        || group_shift >= usize::BITS
    {
        Err(DiscoverError::UnsupportedDevice)
    } else {
        Ok(())
    }
}

fn file_address(
    base: usize,
    file_index: u32,
    hart_bits: u32,
    group_bits: u32,
    group_shift: u32,
) -> Result<usize, DiscoverError> {
    let topology_bits = hart_bits
        .checked_add(group_bits)
        .ok_or(DiscoverError::DeviceRange)?;
    let capacity = 1usize
        .checked_shl(topology_bits)
        .ok_or(DiscoverError::DeviceRange)?;
    let file_index = usize::try_from(file_index).map_err(|_| DiscoverError::DeviceRange)?;
    if file_index >= capacity {
        return Err(DiscoverError::UnsupportedDevice);
    }
    let hart_mask = 1usize
        .checked_shl(hart_bits)
        .ok_or(DiscoverError::DeviceRange)?
        .wrapping_sub(1);
    let hart_offset = (file_index & hart_mask)
        .checked_shl(12)
        .ok_or(DiscoverError::DeviceRange)?;
    let group_offset = (file_index >> hart_bits)
        .checked_shl(group_shift)
        .ok_or(DiscoverError::DeviceRange)?;
    base.checked_add(hart_offset)
        .and_then(|address| address.checked_add(group_offset))
        .ok_or(DiscoverError::DeviceRange)
}
