//! Exact compilation of machine-owned ranges into PMP entries.

use alloc::vec::Vec;

use super::state::*;

pub(super) fn compile(
    machine_ranges: &[Region],
    capability: Capability,
    trusted_without_pmp: bool,
) -> Result<Image, PmpError> {
    if capability.entries == 0 {
        return trusted_without_pmp
            .then_some(Image::TrustedWithoutPmp)
            .ok_or(PmpError::PmpRequired);
    }

    let ranges = normalize(machine_ranges, capability)?;
    let mut entries = Vec::new();
    for range in ranges {
        decompose(range, capability, &mut entries)?;
    }
    entries.push(Entry {
        address: capability.napot_address_mask,
        permissions: Permissions::all(),
        mode: AddressMode::NaturallyAlignedPowerOfTwo,
    });
    if entries.len() > capability.entries {
        return Err(PmpError::InsufficientEntries);
    }
    Ok(Image::Protected(entries))
}

pub(super) fn compile_machine_policy(
    image: Region,
    machine_only_ranges: &[Region],
    capability: Capability,
    trusted_without_pmp: bool,
) -> Result<Image, PmpError> {
    let mut mandatory = Vec::with_capacity(machine_only_ranges.len() + 1);
    mandatory.push(image);
    mandatory.extend_from_slice(machine_only_ranges);
    compile(&mandatory, capability, trusted_without_pmp)
}

fn normalize(ranges: &[Region], capability: Capability) -> Result<Vec<Region>, PmpError> {
    let mut ranges = ranges.to_vec();
    ranges.sort_unstable_by_key(|range| (range.start, range.end));
    let mut normalized: Vec<Region> = Vec::new();
    for range in ranges {
        validate_region(range, capability)?;
        match normalized.last_mut() {
            Some(previous) if range.start <= previous.end => {
                previous.end = previous.end.max(range.end);
            }
            _ => normalized.push(range),
        }
    }
    Ok(normalized)
}

fn validate_region(region: Region, capability: Capability) -> Result<(), PmpError> {
    if region.start >= region.end
        || !region.start.is_multiple_of(4)
        || !region.end.is_multiple_of(4)
    {
        return Err(PmpError::InvalidRegion);
    }
    // `pmpaddr` can represent more physical-address bits than XLEN on RV32.
    // Validate the actual encoded word address against the probed WARL mask
    // instead of deriving a bound from `usize::BITS`.
    if ((region.end - 1) >> 2) & !capability.napot_address_mask != 0 {
        return Err(PmpError::AddressOutOfRange);
    }
    Ok(())
}

fn decompose(
    region: Region,
    capability: Capability,
    entries: &mut Vec<Entry>,
) -> Result<(), PmpError> {
    let mut start = region.start;
    while start < region.end {
        let remaining = region.end - start;
        let remaining_block = highest_power_of_two(remaining);
        let aligned_block = if start == 0 {
            remaining_block
        } else {
            1usize << start.trailing_zeros()
        };
        let size = remaining_block.min(aligned_block);
        if size < capability.granularity {
            return Err(PmpError::Unrepresentable);
        }
        let entry = encode_block(start, size)?;
        if entry.address & !capability.napot_address_mask != 0 {
            return Err(PmpError::AddressOutOfRange);
        }
        entries.push(entry);
        if entries.len() >= MAX_PMP_ENTRIES {
            return Err(PmpError::InsufficientEntries);
        }
        start = start.checked_add(size).ok_or(PmpError::AddressOutOfRange)?;
    }
    Ok(())
}

fn highest_power_of_two(value: usize) -> usize {
    1usize << (usize::BITS - 1 - value.leading_zeros())
}

fn encode_block(start: usize, size: usize) -> Result<Entry, PmpError> {
    if !size.is_power_of_two() || !start.is_multiple_of(size) || size < 4 {
        return Err(PmpError::Unrepresentable);
    }
    if size == 4 {
        return Ok(Entry {
            address: start >> 2,
            permissions: Permissions::empty(),
            mode: AddressMode::NaturallyAlignedFourBytes,
        });
    }
    Ok(Entry {
        address: (start >> 2) | ((size >> 3) - 1),
        permissions: Permissions::empty(),
        mode: AddressMode::NaturallyAlignedPowerOfTwo,
    })
}
