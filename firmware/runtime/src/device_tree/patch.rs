//! Checked binary-FDT rewriting for the next-stage platform description.
//!
//! Header, token, and string-table manipulation is private to Runtime. The
//! module uses only safe Rust and rejects malformed input. The binary layout
//! follows Devicetree Specification v0.4, section 5.2:
//! <https://github.com/devicetree-org/devicetree-specification/releases/tag/v0.4>.

#![forbid(unsafe_code)]

use alloc::{format, vec::Vec};
use core::mem::size_of;

use crate::{Error, Result};

/// A static physical-memory reservation to expose to the next stage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Reservation {
    address: u64,
    size: u64,
}

impl Reservation {
    /// Creates a non-empty reservation.
    pub(super) const fn new(address: u64, size: u64) -> Option<Self> {
        if size == 0 {
            None
        } else {
            Some(Self { address, size })
        }
    }

    /// Returns the reservation start address.
    const fn address(self) -> u64 {
        self.address
    }

    /// Returns the reservation size in bytes.
    const fn size(self) -> u64 {
        self.size
    }
}

mod format;
mod plan;
mod writer;

use format::Layout;
use plan::EditPlan;

/// Validates the complete DTB before Runtime retains its address.
pub(super) fn validate(source: &[u8]) -> Result<()> {
    let layout = Layout::parse(source)?;
    plan::validate_structure(layout.structure, layout.strings)
}

/// Returns a validated DTB with the requested next-stage edits.
pub(super) fn prepare_next_stage(
    source: &[u8],
    firmware_reservation: Option<Reservation>,
    hidden_node_paths: &[&str],
) -> Result<Vec<u8>> {
    if hidden_node_paths
        .iter()
        .any(|path| !path.starts_with('/') || *path == "/" || path.as_bytes().contains(&0))
    {
        return Err(Error::InvalidArgs);
    }

    let layout = Layout::parse(source)?;
    let reservation_name =
        firmware_reservation.map(|reservation| format!("mmode_resv1@{:x}", reservation.address()));
    let edits = plan::plan_edits(
        layout.structure,
        layout.strings,
        hidden_node_paths,
        reservation_name.as_deref(),
    )?;
    let mut structure = layout.structure.to_vec();
    nop_nodes(&mut structure, &edits.hidden_nodes)?;

    if let (Some(reservation), Some(name)) = (firmware_reservation, reservation_name.as_deref()) {
        add_firmware_reservation(&layout, &edits, &structure, reservation, name)
    } else {
        let mut output = source[..layout.total_size].to_vec();
        let end = layout
            .structure_offset
            .checked_add(structure.len())
            .ok_or(Error::Overflow)?;
        output
            .get_mut(layout.structure_offset..end)
            .ok_or(Error::InvalidArgs)?
            .copy_from_slice(&structure);
        Ok(output)
    }
}

fn add_firmware_reservation(
    layout: &Layout<'_>,
    edits: &EditPlan,
    structure: &[u8],
    reservation: Reservation,
    child_name: &str,
) -> Result<Vec<u8>> {
    if let Some(reserved) = edits.reserved_memory.as_ref()
        && (!reserved.accepts_firmware_child
            || reserved.cells != edits.root_cells
            || edits.reservation_exists)
    {
        return Err(Error::InvalidArgs);
    }
    let cells = edits
        .reserved_memory
        .as_ref()
        .map(|node| node.cells)
        .unwrap_or(edits.root_cells);
    plan::validate_cell_widths(cells)?;

    let mut strings = layout.strings.to_vec();
    let address_cells_name = writer::append_string(&mut strings, "#address-cells")?;
    let size_cells_name = writer::append_string(&mut strings, "#size-cells")?;
    let ranges_name = writer::append_string(&mut strings, "ranges")?;
    let reg_name = writer::append_string(&mut strings, "reg")?;
    let no_map_name = writer::append_string(&mut strings, "no-map")?;

    let mut child = Vec::new();
    writer::push_begin_node(&mut child, child_name)?;
    let mut encoded_range = Vec::new();
    writer::push_cells(
        &mut encoded_range,
        reservation.address(),
        cells.address_cells,
    )?;
    writer::push_cells(&mut encoded_range, reservation.size(), cells.size_cells)?;
    writer::push_property(&mut child, reg_name, &encoded_range)?;
    writer::push_property(&mut child, no_map_name, &[])?;
    writer::push_u32(&mut child, format::END_NODE);

    let (insertion_offset, insertion) = if let Some(reserved) = edits.reserved_memory.as_ref() {
        (reserved.end, child)
    } else {
        let mut parent = Vec::new();
        writer::push_begin_node(&mut parent, "reserved-memory")?;
        writer::push_property(
            &mut parent,
            address_cells_name,
            &cells.address_cells.to_be_bytes(),
        )?;
        writer::push_property(
            &mut parent,
            size_cells_name,
            &cells.size_cells.to_be_bytes(),
        )?;
        writer::push_property(&mut parent, ranges_name, &[])?;
        parent.extend_from_slice(&child);
        writer::push_u32(&mut parent, format::END_NODE);
        (edits.root_end, parent)
    };

    let structure_size = structure
        .len()
        .checked_add(insertion.len())
        .ok_or(Error::Overflow)?;
    let mut rewritten = Vec::with_capacity(structure_size);
    rewritten.extend_from_slice(&structure[..insertion_offset]);
    rewritten.extend_from_slice(&insertion);
    rewritten.extend_from_slice(&structure[insertion_offset..]);
    writer::rebuild(layout, &rewritten, &strings)
}

fn nop_nodes(structure: &mut [u8], spans: &[core::ops::Range<usize>]) -> Result<()> {
    for span in spans {
        for word in structure
            .get_mut(span.clone())
            .ok_or(Error::InvalidArgs)?
            .chunks_exact_mut(size_of::<u32>())
        {
            word.copy_from_slice(&format::NOP.to_be_bytes());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
