//! Narrow helpers for validating exact device-tree device identities.

use alloc::vec::Vec;
use core::ops::Range;

use dtoolkit::fdt::{Fdt, FdtNode};
use dtoolkit::{Node, Property};

use crate::config::HART_CAPACITY;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BindingError {
    DeviceTree,
    Unsupported,
    InvalidRange,
}

pub(crate) fn exact_node<'a>(fdt: &Fdt<'a>, path: &str) -> Result<FdtNode<'a>, BindingError> {
    let mut current = fdt.root();
    let mut segments = path
        .strip_prefix('/')
        .filter(|path| !path.is_empty())
        .ok_or(BindingError::DeviceTree)?
        .split('/');
    for segment in segments.by_ref() {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(BindingError::DeviceTree);
        }
        let child = current
            .children()
            .find(|node| node.name() == segment)
            .ok_or(BindingError::DeviceTree)?;
        if current.name() != ""
            && current
                .property("ranges")
                .is_some_and(|ranges| !ranges.value().is_empty())
        {
            // General bus-address translation is deliberately outside the
            // retained concrete-driver contract.
            return Err(BindingError::Unsupported);
        }
        current = child;
    }
    Ok(current)
}

pub(crate) fn enabled(node: &FdtNode<'_>) -> bool {
    node.property("status")
        .and_then(|property| property.as_str().ok())
        .is_none_or(|status| matches!(status, "ok" | "okay"))
}

pub(crate) fn compatible(node: &FdtNode<'_>, accepted: &[&str]) -> bool {
    node.property("compatible").is_some_and(|property| {
        property
            .as_str_list()
            .any(|value| accepted.contains(&value))
    })
}

pub(crate) fn model<'a>(fdt: &Fdt<'a>) -> &'a str {
    fdt.root()
        .property("model")
        .and_then(|property| property.as_str().ok())
        .unwrap_or_default()
}

pub(crate) fn reg_ranges(node: FdtNode<'_>) -> Result<Vec<Range<usize>>, BindingError> {
    let registers = node
        .reg()
        .map_err(|_| BindingError::InvalidRange)?
        .ok_or(BindingError::InvalidRange)?;
    let mut ranges = Vec::new();
    for register in registers {
        let start = register
            .address::<u64>()
            .map_err(|_| BindingError::InvalidRange)?;
        let size = register
            .size::<u64>()
            .map_err(|_| BindingError::InvalidRange)?;
        let start = usize::try_from(start).map_err(|_| BindingError::InvalidRange)?;
        let size = usize::try_from(size).map_err(|_| BindingError::InvalidRange)?;
        let end = start.checked_add(size).ok_or(BindingError::InvalidRange)?;
        if start == end {
            return Err(BindingError::InvalidRange);
        }
        ranges.push(start..end);
    }
    if ranges.is_empty() {
        Err(BindingError::InvalidRange)
    } else {
        Ok(ranges)
    }
}

pub(crate) fn u32_property(node: &FdtNode<'_>, name: &str) -> Result<u32, BindingError> {
    node.property(name)
        .ok_or(BindingError::Unsupported)?
        .as_u32()
        .map_err(|_| BindingError::Unsupported)
}

pub(crate) fn optional_u32_property(
    node: &FdtNode<'_>,
    name: &str,
    default: u32,
) -> Result<u32, BindingError> {
    node.property(name)
        .map(|property| property.as_u32().map_err(|_| BindingError::Unsupported))
        .unwrap_or(Ok(default))
}

pub(crate) fn u32_cells(node: &FdtNode<'_>, name: &str) -> Result<Vec<u32>, BindingError> {
    let property = node.property(name).ok_or(BindingError::Unsupported)?;
    let mut chunks = property.value().chunks_exact(4);
    let cells = chunks
        .by_ref()
        .map(|cell| u32::from_be_bytes([cell[0], cell[1], cell[2], cell[3]]))
        .collect();
    if chunks.remainder().is_empty() {
        Ok(cells)
    } else {
        Err(BindingError::Unsupported)
    }
}

pub(crate) fn cpu_interrupt_controllers(fdt: &Fdt<'_>) -> Result<Vec<(u32, usize)>, BindingError> {
    let cpus = fdt.root().child("cpus").ok_or(BindingError::Unsupported)?;
    let address_cells = cpus
        .property("#address-cells")
        .map(|property| property.as_u32().map_err(|_| BindingError::Unsupported))
        .unwrap_or(Ok(1))?;
    let address_cells = usize::try_from(address_cells).map_err(|_| BindingError::Unsupported)?;
    let mut result = Vec::new();
    for cpu in cpus.children() {
        if cpu.name_without_address() != "cpu" || !enabled(&cpu) {
            continue;
        }
        let hart_cells = u32_cells(&cpu, "reg")?;
        if hart_cells.len() != address_cells || hart_cells.len() > 2 {
            return Err(BindingError::Unsupported);
        }
        // Device-tree cells are 32 bits even when this firmware is built for
        // RV32. Accumulating directly in `usize` would shift an RV32 value by
        // its full width before reading the first cell and reject every CPU
        // interrupt controller. Decode first, then check the machine width.
        let hart_id = hart_cells.iter().try_fold(0u64, |value, cell| {
            value
                .checked_shl(32)
                .map(|value| value | u64::from(*cell))
                .ok_or(BindingError::Unsupported)
        })?;
        let hart_id = usize::try_from(hart_id).map_err(|_| BindingError::Unsupported)?;
        for intc in cpu.children() {
            if !compatible(&intc, &["riscv,cpu-intc"]) {
                continue;
            }
            let phandle = intc
                .property("phandle")
                .or_else(|| intc.property("linux,phandle"))
                .ok_or(BindingError::Unsupported)?
                .as_u32()
                .map_err(|_| BindingError::Unsupported)?;
            if result.iter().any(|(known, _)| *known == phandle) {
                return Err(BindingError::Unsupported);
            }
            result.push((phandle, hart_id));
        }
    }
    if result.is_empty() {
        Err(BindingError::Unsupported)
    } else {
        Ok(result)
    }
}

pub(crate) fn hart_ids(fdt: &Fdt<'_>) -> Result<Vec<usize>, BindingError> {
    let cpus = fdt.root().child("cpus").ok_or(BindingError::DeviceTree)?;
    let address_cells = cpus
        .property("#address-cells")
        .map(|property| property.as_u32().map_err(|_| BindingError::Unsupported))
        .unwrap_or(Ok(1))?;
    let address_cells = usize::try_from(address_cells).map_err(|_| BindingError::Unsupported)?;
    if address_cells == 0 || address_cells > 2 {
        return Err(BindingError::Unsupported);
    }

    let mut ids = Vec::new();
    for cpu in cpus.children() {
        if cpu.name_without_address() != "cpu" || !enabled(&cpu) {
            continue;
        }
        if ids.len() == HART_CAPACITY {
            return Err(BindingError::Unsupported);
        }
        let cells = u32_cells(&cpu, "reg")?;
        if cells.len() != address_cells {
            return Err(BindingError::DeviceTree);
        }
        // DT cells are always 32 bits, independently of firmware XLEN. Decode
        // in a fixed-width accumulator before checking that the physical hart
        // ID is representable by this machine image.
        let id = cells.into_iter().try_fold(0u64, |value, cell| {
            value
                .checked_shl(32)
                .map(|value| value | u64::from(cell))
                .ok_or(BindingError::Unsupported)
        })?;
        let id = usize::try_from(id).map_err(|_| BindingError::Unsupported)?;
        if ids.contains(&id) {
            return Err(BindingError::DeviceTree);
        }
        ids.push(id);
    }
    (!ids.is_empty())
        .then_some(ids)
        .ok_or(BindingError::DeviceTree)
}
