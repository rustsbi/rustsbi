//! Checked access to the version-17 DTB binary format.
//!
//! The header and structure layout follow the Devicetree Specification v0.4,
//! section 5.2: <https://github.com/devicetree-org/devicetree-specification/releases/tag/v0.4>.

use core::mem::size_of;

use crate::{Error, Result};

pub(in crate::device_tree::patch) const MAGIC: u32 = 0xd00d_feed;
pub(in crate::device_tree::patch) const HEADER_SIZE: usize = 10 * size_of::<u32>();
pub(in crate::device_tree::patch) const RESERVATION_ENTRY_SIZE: usize = 2 * size_of::<u64>();

const MAGIC_OFFSET: usize = 0;
pub(in crate::device_tree::patch) const TOTAL_SIZE_OFFSET: usize = 4;
pub(in crate::device_tree::patch) const STRUCTURE_OFFSET_OFFSET: usize = 8;
pub(in crate::device_tree::patch) const STRINGS_OFFSET_OFFSET: usize = 12;
pub(in crate::device_tree::patch) const RESERVATIONS_OFFSET_OFFSET: usize = 16;
pub(in crate::device_tree::patch) const VERSION_OFFSET: usize = 20;
pub(in crate::device_tree::patch) const LAST_COMPATIBLE_VERSION_OFFSET: usize = 24;
pub(in crate::device_tree::patch) const STRINGS_SIZE_OFFSET: usize = 32;
pub(in crate::device_tree::patch) const STRUCTURE_SIZE_OFFSET: usize = 36;
/// The DTB header version currently accepted by Runtime.
// FIXME: Add version-aware parsing or normalization for older supported FDT
// header versions before widening this contract.
pub(in crate::device_tree::patch) const SUPPORTED_VERSION: u32 = 17;

pub(in crate::device_tree::patch) const BEGIN_NODE: u32 = 1;
pub(in crate::device_tree::patch) const END_NODE: u32 = 2;
pub(in crate::device_tree::patch) const PROPERTY: u32 = 3;
pub(in crate::device_tree::patch) const NOP: u32 = 4;
pub(in crate::device_tree::patch) const END: u32 = 9;

pub(in crate::device_tree::patch) const DEFAULT_ADDRESS_CELLS: u32 = 2;
pub(in crate::device_tree::patch) const DEFAULT_SIZE_CELLS: u32 = 1;

pub(in crate::device_tree::patch) struct Layout<'a> {
    pub(in crate::device_tree::patch) source: &'a [u8],
    pub(in crate::device_tree::patch) total_size: usize,
    pub(in crate::device_tree::patch) reservations: &'a [u8],
    pub(in crate::device_tree::patch) structure_offset: usize,
    pub(in crate::device_tree::patch) structure: &'a [u8],
    pub(in crate::device_tree::patch) strings: &'a [u8],
}

impl<'a> Layout<'a> {
    pub(in crate::device_tree::patch) fn parse(source: &'a [u8]) -> Result<Self> {
        if source.len() < HEADER_SIZE || read_u32(source, MAGIC_OFFSET)? != MAGIC {
            return Err(Error::InvalidArgs);
        }
        let total_size = read_usize(source, TOTAL_SIZE_OFFSET)?;
        if !(HEADER_SIZE..=source.len()).contains(&total_size) {
            return Err(Error::InvalidArgs);
        }
        let version = read_u32(source, VERSION_OFFSET)?;
        let last_compatible_version = read_u32(source, LAST_COMPATIBLE_VERSION_OFFSET)?;
        if version != SUPPORTED_VERSION || last_compatible_version > version {
            return Err(Error::InvalidArgs);
        }

        let structure_offset = read_usize(source, STRUCTURE_OFFSET_OFFSET)?;
        let strings_offset = read_usize(source, STRINGS_OFFSET_OFFSET)?;
        let reservations_offset = read_usize(source, RESERVATIONS_OFFSET_OFFSET)?;
        let structure_size = read_usize(source, STRUCTURE_SIZE_OFFSET)?;
        let strings_size = read_usize(source, STRINGS_SIZE_OFFSET)?;
        if !structure_offset.is_multiple_of(4)
            || !strings_offset.is_multiple_of(4)
            || !reservations_offset.is_multiple_of(8)
        {
            return Err(Error::InvalidArgs);
        }

        let structure = checked_slice(source, structure_offset, structure_size, total_size)?;
        let strings = checked_slice(source, strings_offset, strings_size, total_size)?;
        let reservations_end = reservation_map_end(source, reservations_offset, total_size)?;
        let structure_end = structure_offset
            .checked_add(structure_size)
            .ok_or(Error::Overflow)?;
        let strings_end = strings_offset
            .checked_add(strings_size)
            .ok_or(Error::Overflow)?;
        if reservations_offset < HEADER_SIZE
            || reservations_end > structure_offset
            || structure_end > strings_offset
            || strings_end > total_size
        {
            return Err(Error::InvalidArgs);
        }
        let reservations = &source[reservations_offset..reservations_end];

        Ok(Self {
            source,
            total_size,
            reservations,
            structure_offset,
            structure,
            strings,
        })
    }
}

fn reservation_map_end(source: &[u8], start: usize, total_size: usize) -> Result<usize> {
    if start < HEADER_SIZE || !start.is_multiple_of(8) {
        return Err(Error::InvalidArgs);
    }
    let mut offset = start;
    loop {
        let entry = checked_slice(source, offset, RESERVATION_ENTRY_SIZE, total_size)?;
        offset = offset
            .checked_add(RESERVATION_ENTRY_SIZE)
            .ok_or(Error::Overflow)?;
        if entry.iter().all(|byte| *byte == 0) {
            return Ok(offset);
        }
    }
}

pub(in crate::device_tree::patch) fn take_u32(bytes: &[u8], offset: &mut usize) -> Result<u32> {
    let value = read_u32(bytes, *offset)?;
    *offset = offset
        .checked_add(size_of::<u32>())
        .ok_or(Error::Overflow)?;
    Ok(value)
}

pub(in crate::device_tree::patch) fn take_c_string<'a>(
    bytes: &'a [u8],
    offset: &mut usize,
) -> Result<&'a str> {
    let tail = bytes.get(*offset..).ok_or(Error::InvalidArgs)?;
    let length = tail
        .iter()
        .position(|byte| *byte == 0)
        .ok_or(Error::InvalidArgs)?;
    let name = core::str::from_utf8(&tail[..length]).map_err(|_| Error::InvalidArgs)?;
    let encoded_length = length.checked_add(1).ok_or(Error::Overflow)?;
    *offset = offset
        .checked_add(align_up(encoded_length, 4)?)
        .ok_or(Error::Overflow)?;
    if *offset > bytes.len() {
        return Err(Error::InvalidArgs);
    }
    Ok(name)
}

pub(in crate::device_tree::patch) fn take_aligned<'a>(
    bytes: &'a [u8],
    offset: &mut usize,
    length: usize,
) -> Result<&'a [u8]> {
    let value = bytes
        .get(*offset..offset.checked_add(length).ok_or(Error::Overflow)?)
        .ok_or(Error::InvalidArgs)?;
    *offset = offset
        .checked_add(align_up(length, 4)?)
        .ok_or(Error::Overflow)?;
    if *offset > bytes.len() {
        return Err(Error::InvalidArgs);
    }
    Ok(value)
}

pub(in crate::device_tree::patch) fn string_at(strings: &[u8], offset: usize) -> Result<&str> {
    let tail = strings.get(offset..).ok_or(Error::InvalidArgs)?;
    let length = tail
        .iter()
        .position(|byte| *byte == 0)
        .ok_or(Error::InvalidArgs)?;
    core::str::from_utf8(&tail[..length]).map_err(|_| Error::InvalidArgs)
}

fn checked_slice(source: &[u8], offset: usize, length: usize, total_size: usize) -> Result<&[u8]> {
    let end = offset.checked_add(length).ok_or(Error::Overflow)?;
    if end > total_size {
        return Err(Error::InvalidArgs);
    }
    source.get(offset..end).ok_or(Error::InvalidArgs)
}

pub(in crate::device_tree::patch) fn read_usize(bytes: &[u8], offset: usize) -> Result<usize> {
    usize::try_from(read_u32(bytes, offset)?).map_err(|_| Error::Overflow)
}

pub(in crate::device_tree::patch) fn read_u32(bytes: &[u8], offset: usize) -> Result<u32> {
    let encoded: [u8; 4] = bytes
        .get(offset..offset.checked_add(4).ok_or(Error::Overflow)?)
        .ok_or(Error::InvalidArgs)?
        .try_into()
        .map_err(|_| Error::InvalidArgs)?;
    Ok(u32::from_be_bytes(encoded))
}

pub(in crate::device_tree::patch) fn align_up(value: usize, alignment: usize) -> Result<usize> {
    value
        .checked_add(alignment - 1)
        .map(|value| value & !(alignment - 1))
        .ok_or(Error::Overflow)
}
