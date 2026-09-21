//! DTB token, string-table, and header emission.
//!
//! The emitted layout follows Devicetree Specification v0.4, section 5.2:
//! <https://github.com/devicetree-org/devicetree-specification/releases/tag/v0.4>.

use alloc::vec::Vec;

use super::format::{
    BEGIN_NODE, HEADER_SIZE, Layout, PROPERTY, RESERVATIONS_OFFSET_OFFSET, STRINGS_OFFSET_OFFSET,
    STRINGS_SIZE_OFFSET, STRUCTURE_OFFSET_OFFSET, STRUCTURE_SIZE_OFFSET, TOTAL_SIZE_OFFSET,
};
use crate::{Error, Result};

pub(in crate::device_tree::patch) fn push_cells(
    output: &mut Vec<u8>,
    value: u64,
    count: u32,
) -> Result<()> {
    match count {
        1 => {
            let value = u32::try_from(value).map_err(|_| Error::Overflow)?;
            output.extend_from_slice(&value.to_be_bytes());
        }
        2 => {
            output.extend_from_slice(&((value >> 32) as u32).to_be_bytes());
            output.extend_from_slice(&(value as u32).to_be_bytes());
        }
        _ => return Err(Error::InvalidArgs),
    }
    Ok(())
}

pub(in crate::device_tree::patch) fn push_begin_node(
    output: &mut Vec<u8>,
    name: &str,
) -> Result<()> {
    if name.as_bytes().contains(&0) {
        return Err(Error::InvalidArgs);
    }
    push_u32(output, BEGIN_NODE);
    output.extend_from_slice(name.as_bytes());
    output.push(0);
    pad_to(output, 4);
    Ok(())
}

pub(in crate::device_tree::patch) fn push_property(
    output: &mut Vec<u8>,
    name_offset: u32,
    value: &[u8],
) -> Result<()> {
    push_u32(output, PROPERTY);
    push_u32(
        output,
        u32::try_from(value.len()).map_err(|_| Error::Overflow)?,
    );
    push_u32(output, name_offset);
    output.extend_from_slice(value);
    pad_to(output, 4);
    Ok(())
}

pub(in crate::device_tree::patch) fn append_string(
    strings: &mut Vec<u8>,
    name: &str,
) -> Result<u32> {
    let offset = u32::try_from(strings.len()).map_err(|_| Error::Overflow)?;
    strings.extend_from_slice(name.as_bytes());
    strings.push(0);
    Ok(offset)
}

pub(in crate::device_tree::patch) fn rebuild(
    layout: &Layout<'_>,
    structure: &[u8],
    strings: &[u8],
) -> Result<Vec<u8>> {
    let mut output = layout.source[..HEADER_SIZE].to_vec();
    pad_to(&mut output, 8);
    let reservations_offset = output.len();
    output.extend_from_slice(layout.reservations);
    pad_to(&mut output, 4);
    let structure_offset = output.len();
    output.extend_from_slice(structure);
    let strings_offset = output.len();
    output.extend_from_slice(strings);
    pad_to(&mut output, 4);

    let total_size = output.len();
    write_header_field(&mut output, TOTAL_SIZE_OFFSET, total_size)?;
    write_header_field(&mut output, STRUCTURE_OFFSET_OFFSET, structure_offset)?;
    write_header_field(&mut output, STRINGS_OFFSET_OFFSET, strings_offset)?;
    write_header_field(&mut output, RESERVATIONS_OFFSET_OFFSET, reservations_offset)?;
    write_header_field(&mut output, STRINGS_SIZE_OFFSET, strings.len())?;
    write_header_field(&mut output, STRUCTURE_SIZE_OFFSET, structure.len())?;
    Ok(output)
}

pub(in crate::device_tree::patch) fn write_header_field(
    output: &mut [u8],
    offset: usize,
    value: usize,
) -> Result<()> {
    let value = u32::try_from(value).map_err(|_| Error::Overflow)?;
    output
        .get_mut(offset..offset.checked_add(4).ok_or(Error::Overflow)?)
        .ok_or(Error::InvalidArgs)?
        .copy_from_slice(&value.to_be_bytes());
    Ok(())
}

pub(in crate::device_tree::patch) fn push_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_be_bytes());
}

pub(in crate::device_tree::patch) fn pad_to(output: &mut Vec<u8>, alignment: usize) {
    output.resize(output.len().next_multiple_of(alignment), 0);
}
