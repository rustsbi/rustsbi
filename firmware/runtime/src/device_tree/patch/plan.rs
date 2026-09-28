//! One-pass validation and planning of next-stage DTB edits.

use alloc::{string::String, vec::Vec};
use core::{mem::size_of, ops::Range};

use super::format::{
    BEGIN_NODE, DEFAULT_ADDRESS_CELLS, DEFAULT_SIZE_CELLS, END, END_NODE, NOP, PROPERTY, read_u32,
    string_at, take_aligned, take_c_string, take_u32,
};
use crate::{Error, Result};

/// Number of 32-bit cells used for an address and a size in a `reg` tuple.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(in crate::device_tree::patch) struct CellCounts {
    pub(in crate::device_tree::patch) address_cells: u32,
    pub(in crate::device_tree::patch) size_cells: u32,
}

/// Maximum nesting depth accepted by the `fdt` traversal used by Runtime.
// `fdt 0.1.5` stores the parent nodes of `Fdt::all_nodes` in a fixed
// `[&[u8]; 64]` array and indexes it with the one-based nesting depth. Keep
// validated input below that parser limit before any runtime code traverses it.
pub(in crate::device_tree::patch) const MAX_FDT_DEPTH: usize = 63;

#[derive(Clone, Copy)]
struct ValidationNode {
    address_cells: Option<u32>,
    size_cells: Option<u32>,
    has_child: bool,
}

impl ValidationNode {
    const fn new() -> Self {
        Self {
            address_cells: None,
            size_cells: None,
            has_child: false,
        }
    }

    fn cells(self) -> CellCounts {
        CellCounts {
            address_cells: self.address_cells.unwrap_or(DEFAULT_ADDRESS_CELLS),
            size_cells: self.size_cells.unwrap_or(DEFAULT_SIZE_CELLS),
        }
    }
}

/// Offsets and format facts needed by the two supported next-stage edits.
pub(in crate::device_tree::patch) struct EditPlan {
    pub(in crate::device_tree::patch) root_end: usize,
    pub(in crate::device_tree::patch) root_cells: CellCounts,
    pub(in crate::device_tree::patch) reserved_memory: Option<ReservedMemory>,
    pub(in crate::device_tree::patch) reservation_exists: bool,
    pub(in crate::device_tree::patch) hidden_nodes: Vec<Range<usize>>,
}

pub(in crate::device_tree::patch) struct ReservedMemory {
    pub(in crate::device_tree::patch) end: usize,
    pub(in crate::device_tree::patch) cells: CellCounts,
    pub(in crate::device_tree::patch) accepts_firmware_child: bool,
}

struct OpenNode {
    start: usize,
    previous_path_length: usize,
    hide: bool,
    is_root: bool,
    is_reserved_memory: bool,
    is_reservation: bool,
    address_cells: Option<u32>,
    size_cells: Option<u32>,
    enabled: bool,
    empty_ranges: bool,
}

struct EditPlanner {
    stack: Vec<OpenNode>,
    path: String,
    root_end: Option<usize>,
    root_cells: Option<CellCounts>,
    reserved_memory: Option<ReservedMemory>,
    reservation_exists: bool,
    hidden_nodes: Vec<Range<usize>>,
}

impl OpenNode {
    fn record_property(&mut self, name: &str, value: &[u8]) -> Result<()> {
        match name {
            "#address-cells" => self.address_cells = Some(single_cell(value)?),
            "#size-cells" => self.size_cells = Some(single_cell(value)?),
            "ranges" if self.is_reserved_memory => self.empty_ranges = value.is_empty(),
            "status" if self.is_reserved_memory => {
                let status = value.strip_suffix(&[0]).unwrap_or(value);
                self.enabled = matches!(status, b"ok" | b"okay");
            }
            _ => {}
        }
        Ok(())
    }

    fn cells(&self) -> CellCounts {
        CellCounts {
            address_cells: self.address_cells.unwrap_or(DEFAULT_ADDRESS_CELLS),
            size_cells: self.size_cells.unwrap_or(DEFAULT_SIZE_CELLS),
        }
    }
}

impl EditPlanner {
    fn new() -> Self {
        Self {
            stack: Vec::new(),
            path: String::new(),
            root_end: None,
            root_cells: None,
            reserved_memory: None,
            reservation_exists: false,
            hidden_nodes: Vec::new(),
        }
    }

    fn begin_node(
        &mut self,
        start: usize,
        name: &str,
        hidden_node_paths: &[&str],
        reservation_name: Option<&str>,
    ) -> Result<()> {
        if self.root_end.is_some() {
            return Err(Error::InvalidArgs);
        }
        let is_root = self.stack.is_empty();
        if is_root && !name.is_empty() {
            return Err(Error::InvalidArgs);
        }

        let previous_path_length = self.path.len();
        if is_root {
            self.path.push('/');
        } else {
            if self.path.len() > 1 {
                self.path.push('/');
            }
            self.path.push_str(name);
        }
        let is_reserved_memory =
            self.stack.len() == 1 && name.split('@').next() == Some("reserved-memory");
        let is_reservation = self
            .stack
            .last()
            .is_some_and(|parent| parent.is_reserved_memory)
            && reservation_name == Some(name);
        self.stack.push(OpenNode {
            start,
            previous_path_length,
            hide: hidden_node_paths.contains(&self.path.as_str()),
            is_root,
            is_reserved_memory,
            is_reservation,
            address_cells: None,
            size_cells: None,
            enabled: true,
            empty_ranges: false,
        });
        Ok(())
    }

    fn record_property(&mut self, name: &str, value: &[u8]) -> Result<()> {
        self.stack
            .last_mut()
            .ok_or(Error::InvalidArgs)?
            .record_property(name, value)
    }

    fn end_node(&mut self, token_offset: usize, end: usize) -> Result<()> {
        let node = self.stack.pop().ok_or(Error::InvalidArgs)?;
        let cells = node.cells();
        if node.is_reserved_memory
            && self
                .reserved_memory
                .replace(ReservedMemory {
                    end: token_offset,
                    cells,
                    accepts_firmware_child: node.enabled
                        && node.address_cells.is_some()
                        && node.size_cells.is_some()
                        && node.empty_ranges,
                })
                .is_some()
        {
            return Err(Error::InvalidArgs);
        }
        self.reservation_exists |= node.is_reservation;
        if node.hide {
            self.hidden_nodes.push(node.start..end);
        }
        self.path.truncate(node.previous_path_length);
        if node.is_root {
            if !self.stack.is_empty() {
                return Err(Error::InvalidArgs);
            }
            self.root_end = Some(token_offset);
            self.root_cells = Some(cells);
        }
        Ok(())
    }

    fn finish(self) -> Result<EditPlan> {
        if !self.stack.is_empty() {
            return Err(Error::InvalidArgs);
        }
        Ok(EditPlan {
            root_end: self.root_end.ok_or(Error::InvalidArgs)?,
            root_cells: self.root_cells.ok_or(Error::InvalidArgs)?,
            reserved_memory: self.reserved_memory,
            reservation_exists: self.reservation_exists,
            hidden_nodes: self.hidden_nodes,
        })
    }
}

fn single_cell(value: &[u8]) -> Result<u32> {
    if value.len() != size_of::<u32>() {
        return Err(Error::InvalidArgs);
    }
    read_u32(value, 0)
}

/// Validates structure tokens without heap allocation. This runs at the boot
/// trust seam, before the firmware heap is initialized.
pub(in crate::device_tree::patch) fn validate_structure(
    structure: &[u8],
    strings: &[u8],
) -> Result<()> {
    let mut offset = 0;
    let mut depth = 0usize;
    let mut root_seen = false;
    let mut nodes = [ValidationNode::new(); MAX_FDT_DEPTH];

    while offset < structure.len() {
        let token = take_u32(structure, &mut offset)?;
        match token {
            BEGIN_NODE => {
                let name = take_c_string(structure, &mut offset)?;
                if depth == 0 {
                    if root_seen || !name.is_empty() {
                        return Err(Error::InvalidArgs);
                    }
                    root_seen = true;
                } else {
                    nodes[depth - 1].has_child = true;
                }
                if depth >= MAX_FDT_DEPTH {
                    return Err(Error::InvalidArgs);
                }
                nodes[depth] = ValidationNode::new();
                depth += 1;
            }
            PROPERTY => {
                if depth == 0 {
                    return Err(Error::InvalidArgs);
                }
                if nodes[depth - 1].has_child {
                    return Err(Error::InvalidArgs);
                }
                let length = take_u32(structure, &mut offset)? as usize;
                let name_offset = take_u32(structure, &mut offset)? as usize;
                let value = take_aligned(structure, &mut offset, length)?;
                let name = string_at(strings, name_offset)?;
                let parent_cells = if depth == 1 {
                    CellCounts {
                        address_cells: DEFAULT_ADDRESS_CELLS,
                        size_cells: DEFAULT_SIZE_CELLS,
                    }
                } else {
                    nodes[depth - 2].cells()
                };
                let node = &mut nodes[depth - 1];
                match name {
                    "#address-cells" => {
                        let value = single_cell(value)?;
                        if node.address_cells.replace(value).is_some() {
                            return Err(Error::InvalidArgs);
                        }
                    }
                    "#size-cells" => {
                        let value = single_cell(value)?;
                        if node.size_cells.replace(value).is_some() {
                            return Err(Error::InvalidArgs);
                        }
                    }
                    "reg" => {
                        validate_reg_length(value, parent_cells)?;
                    }
                    _ => {}
                }
            }
            END_NODE => {
                depth = depth.checked_sub(1).ok_or(Error::InvalidArgs)?;
            }
            NOP => {}
            END => {
                if !root_seen || depth != 0 {
                    return Err(Error::InvalidArgs);
                }
                let mut trailing = structure[offset..].chunks_exact(size_of::<u32>());
                if trailing.any(|word| word != NOP.to_be_bytes())
                    || !trailing.remainder().is_empty()
                {
                    return Err(Error::InvalidArgs);
                }
                return Ok(());
            }
            _ => return Err(Error::InvalidArgs),
        }
    }
    Err(Error::InvalidArgs)
}

fn validate_reg_length(value: &[u8], cells: CellCounts) -> Result<()> {
    let tuple_cells = cells
        .address_cells
        .checked_add(cells.size_cells)
        .ok_or(Error::Overflow)?;
    let tuple_bytes = usize::try_from(tuple_cells)
        .map_err(|_| Error::Overflow)?
        .checked_mul(size_of::<u32>())
        .ok_or(Error::Overflow)?;
    if tuple_bytes == 0 || !value.len().is_multiple_of(tuple_bytes) {
        return Err(Error::InvalidArgs);
    }
    Ok(())
}

/// Validates the structure block once while collecting the complete edit plan.
pub(in crate::device_tree::patch) fn plan_edits(
    structure: &[u8],
    strings: &[u8],
    hidden_node_paths: &[&str],
    reservation_name: Option<&str>,
) -> Result<EditPlan> {
    let mut offset = 0;
    let mut planner = EditPlanner::new();

    while offset < structure.len() {
        let token_offset = offset;
        let token = take_u32(structure, &mut offset)?;
        match token {
            BEGIN_NODE => {
                let name = take_c_string(structure, &mut offset)?;
                planner.begin_node(token_offset, name, hidden_node_paths, reservation_name)?;
            }
            PROPERTY => {
                let length = take_u32(structure, &mut offset)? as usize;
                let name_offset = take_u32(structure, &mut offset)? as usize;
                let value = take_aligned(structure, &mut offset, length)?;
                let property_name = string_at(strings, name_offset)?;
                planner.record_property(property_name, value)?;
            }
            END_NODE => planner.end_node(token_offset, offset)?,
            NOP => {}
            END => {
                let mut trailing = structure[offset..].chunks_exact(size_of::<u32>());
                if trailing.any(|word| word != NOP.to_be_bytes())
                    || !trailing.remainder().is_empty()
                {
                    return Err(Error::InvalidArgs);
                }
                return planner.finish();
            }
            _ => return Err(Error::InvalidArgs),
        }
    }
    Err(Error::InvalidArgs)
}

pub(in crate::device_tree::patch) fn validate_cell_widths(cells: CellCounts) -> Result<()> {
    if !(1..=2).contains(&cells.address_cells) || !(1..=2).contains(&cells.size_cells) {
        return Err(Error::InvalidArgs);
    }
    Ok(())
}
