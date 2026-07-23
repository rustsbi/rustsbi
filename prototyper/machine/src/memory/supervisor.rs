//! Checked physical access to next-stage shared memory.
//!
//! The capability does not own, allocate, retype, or borrow external frames.
//! It authorizes byte transfer by value only; no Rust reference is ever formed
//! from the supplied physical address.

use alloc::vec::Vec;
use core::ops::Range;

use dtoolkit::fdt::Fdt;
use dtoolkit::{Node, Property};

use crate::boot::BootInfo;
use crate::boot::device_tree::reg_ranges;
use crate::trap::expected::{ExpectedResult, load_byte, store_byte};

/// Failure while validating or accessing next-stage physical memory.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryError {
    /// The tuple wraps, crosses a hole, or is not authorized normal memory.
    InvalidRange,
    /// The address exceeds the direct physical-address width implemented here.
    UnsupportedRange,
    /// A contained physical access faulted after policy validation.
    Fault,
}

/// Authority to transfer bytes to and from validated next-stage physical RAM.
pub struct SupervisorMemory {
    ranges: Vec<Range<usize>>,
}

impl SupervisorMemory {
    /// Builds the immutable policy from the owned boot description and every
    /// machine resource claimed so far.
    pub(crate) fn from_boot(boot: &BootInfo) -> Result<Self, MemoryError> {
        let fdt = Fdt::new(boot.dtb().as_bytes()).map_err(|_| MemoryError::InvalidRange)?;
        let mut memory = Vec::new();
        for node in fdt.root().children() {
            let is_memory = node.name().split('@').next() == Some("memory")
                || node
                    .property("device_type")
                    .and_then(|property| property.as_str().ok())
                    == Some("memory");
            if is_memory {
                memory.extend(reg_ranges(node).map_err(|_| MemoryError::InvalidRange)?);
            }
        }
        if memory.is_empty() {
            return Err(MemoryError::InvalidRange);
        }

        let mut excluded = boot.machine_ranges().to_vec();
        for reservation in fdt.memory_reservations() {
            let start = usize::try_from(reservation.address())
                .map_err(|_| MemoryError::UnsupportedRange)?;
            let size =
                usize::try_from(reservation.size()).map_err(|_| MemoryError::UnsupportedRange)?;
            let end = start
                .checked_add(size)
                .ok_or(MemoryError::UnsupportedRange)?;
            if start != end {
                excluded.push(start..end);
            }
        }
        if let Some(reservations) = fdt.reserved_memory() {
            for reservation in reservations {
                // A child without a concrete `reg` tuple describes memory
                // whose physical placement is not yet known. Authorizing RAM
                // around such a reservation would make the policy depend on
                // an allocation performed outside this immutable capability,
                // so construction fails closed.
                excluded.extend(reg_ranges(*reservation).map_err(|_| MemoryError::InvalidRange)?);
            }
        }
        excluded.push(crate::pmp::machine_image_range().ok_or(MemoryError::InvalidRange)?);

        let ranges = subtract(memory, excluded)?;
        if ranges.is_empty() {
            return Err(MemoryError::InvalidRange);
        }
        Ok(Self { ranges })
    }

    /// Validates a complete physical range for machine reads.
    pub fn reader(
        &self,
        base_addr_lo: usize,
        base_addr_hi: usize,
        requested_len: usize,
    ) -> Result<Reader<'_>, MemoryError> {
        let range = self.validate(base_addr_lo, base_addr_hi, requested_len)?;
        Ok(Reader {
            memory: self,
            cursor: range.start,
            remaining: requested_len,
        })
    }

    /// Validates a complete physical range for machine writes.
    pub fn writer(
        &self,
        base_addr_lo: usize,
        base_addr_hi: usize,
        requested_len: usize,
    ) -> Result<Writer<'_>, MemoryError> {
        let range = self.validate(base_addr_lo, base_addr_hi, requested_len)?;
        Ok(Writer {
            memory: self,
            cursor: range.start,
            remaining: requested_len,
        })
    }

    fn validate(&self, base: usize, high: usize, len: usize) -> Result<Range<usize>, MemoryError> {
        if len == 0 {
            return Ok(0..0);
        }
        if high != 0 {
            return Err(MemoryError::UnsupportedRange);
        }
        let end = base.checked_add(len).ok_or(MemoryError::InvalidRange)?;
        let range = base..end;
        if !self
            .ranges
            .iter()
            .any(|allowed| allowed.start <= range.start && range.end <= allowed.end)
        {
            return Err(MemoryError::InvalidRange);
        }
        Ok(range)
    }
}

/// One stack-owned cursor for checked physical reads.
pub struct Reader<'memory> {
    memory: &'memory SupervisorMemory,
    cursor: usize,
    remaining: usize,
}

impl Reader<'_> {
    /// Returns the number of bytes left in this validated cursor.
    pub const fn remaining(&self) -> usize {
        self.remaining
    }

    /// Reads exactly `destination.len()` bytes or returns a typed fault.
    pub fn read_exact(&mut self, destination: &mut [u8]) -> Result<(), MemoryError> {
        let end = self.transfer_end(destination.len())?;
        for (offset, output) in destination.iter_mut().enumerate() {
            let address = self
                .cursor
                .checked_add(offset)
                .ok_or(MemoryError::InvalidRange)?;
            // SAFETY: construction validated the complete range as normal RAM;
            // the expected-trap leaf returns only an owned byte or typed fault.
            *output = match unsafe { load_byte(address) } {
                ExpectedResult::Value(value) => value as u8,
                ExpectedResult::Fault(fault) => {
                    let _ = (
                        fault.cause,
                        fault.value,
                        fault.value2,
                        fault.instruction,
                        fault.guest_address,
                    );
                    return Err(MemoryError::Fault);
                }
                ExpectedResult::Busy | ExpectedResult::Unavailable => {
                    return Err(MemoryError::Fault);
                }
            };
        }
        self.cursor = end;
        self.remaining -= destination.len();
        Ok(())
    }

    fn transfer_end(&self, len: usize) -> Result<usize, MemoryError> {
        if len > self.remaining {
            return Err(MemoryError::InvalidRange);
        }
        let end = self
            .cursor
            .checked_add(len)
            .ok_or(MemoryError::InvalidRange)?;
        self.memory.validate(self.cursor, 0, len)?;
        Ok(end)
    }
}

/// One stack-owned cursor for checked physical writes.
pub struct Writer<'memory> {
    memory: &'memory SupervisorMemory,
    cursor: usize,
    remaining: usize,
}

impl Writer<'_> {
    /// Returns the number of bytes left in this validated cursor.
    pub const fn remaining(&self) -> usize {
        self.remaining
    }

    /// Writes exactly `source.len()` bytes or returns a typed fault.
    pub fn write_all(&mut self, source: &[u8]) -> Result<(), MemoryError> {
        let end = self.transfer_end(source.len())?;
        for (offset, byte) in source.iter().copied().enumerate() {
            let address = self
                .cursor
                .checked_add(offset)
                .ok_or(MemoryError::InvalidRange)?;
            // SAFETY: construction validated the complete range as writable
            // normal RAM; no Rust reference is formed over external memory.
            match unsafe { store_byte(address, byte) } {
                ExpectedResult::Value(_) => {}
                ExpectedResult::Fault(fault) => {
                    let _ = (
                        fault.cause,
                        fault.value,
                        fault.value2,
                        fault.instruction,
                        fault.guest_address,
                    );
                    return Err(MemoryError::Fault);
                }
                ExpectedResult::Busy | ExpectedResult::Unavailable => {
                    return Err(MemoryError::Fault);
                }
            }
        }
        self.cursor = end;
        self.remaining -= source.len();
        Ok(())
    }

    fn transfer_end(&self, len: usize) -> Result<usize, MemoryError> {
        if len > self.remaining {
            return Err(MemoryError::InvalidRange);
        }
        let end = self
            .cursor
            .checked_add(len)
            .ok_or(MemoryError::InvalidRange)?;
        self.memory.validate(self.cursor, 0, len)?;
        Ok(end)
    }
}

fn subtract(
    mut memory: Vec<Range<usize>>,
    mut excluded: Vec<Range<usize>>,
) -> Result<Vec<Range<usize>>, MemoryError> {
    if memory.iter().any(|range| range.start >= range.end)
        || excluded.iter().any(|range| range.start >= range.end)
    {
        return Err(MemoryError::InvalidRange);
    }
    memory.sort_unstable_by_key(|range| (range.start, range.end));
    excluded.sort_unstable_by_key(|range| (range.start, range.end));

    let mut result = Vec::new();
    for range in memory {
        let mut cursor = range.start;
        for denied in &excluded {
            if denied.end <= cursor || range.end <= denied.start {
                continue;
            }
            if cursor < denied.start {
                result.push(cursor..denied.start.min(range.end));
            }
            cursor = cursor.max(denied.end);
            if cursor >= range.end {
                break;
            }
        }
        if cursor < range.end {
            result.push(cursor..range.end);
        }
    }
    result.sort_unstable_by_key(|range| (range.start, range.end));
    let mut merged: Vec<Range<usize>> = Vec::new();
    for range in result {
        match merged.last_mut() {
            Some(previous) if range.start <= previous.end => {
                previous.end = previous.end.max(range.end);
            }
            _ => merged.push(range),
        }
    }
    Ok(merged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dtoolkit::model::{DeviceTree, DeviceTreeNode, DeviceTreeProperty};

    fn policy(ranges: &[Range<usize>]) -> SupervisorMemory {
        SupervisorMemory {
            ranges: ranges.to_vec(),
        }
    }

    #[test]
    fn policy_subtraction_excludes_machine_and_reserved_intervals() {
        assert_eq!(
            subtract(
                alloc::vec![0x1000..0x9000],
                alloc::vec![0x2000..0x3000, 0x5000..0x7000]
            ),
            Ok(alloc::vec![0x1000..0x2000, 0x3000..0x5000, 0x7000..0x9000])
        );
    }

    #[test]
    fn constructors_validate_the_complete_range_and_upper_half() {
        let memory = policy(&[0x1000..0x2000, 0x3000..0x4000]);
        assert!(memory.reader(0x1000, 0, 0x1000).is_ok());
        assert_eq!(
            memory.reader(0x1800, 0, 0x1000).err(),
            Some(MemoryError::InvalidRange)
        );
        assert_eq!(
            memory.writer(0x1000, 1, 1).err(),
            Some(MemoryError::UnsupportedRange)
        );
        assert!(memory.writer(usize::MAX, usize::MAX, 0).is_ok());
    }

    #[test]
    fn cursor_rejects_a_slice_larger_than_the_validated_remainder() {
        let range = 0x1000..0x2000;
        let memory = policy(core::slice::from_ref(&range));
        let reader = memory.reader(0x1000, 0, 2).unwrap();
        assert_eq!(reader.transfer_end(3), Err(MemoryError::InvalidRange));
        let writer = memory.writer(0x1000, 0, 2).unwrap();
        assert_eq!(writer.transfer_end(3), Err(MemoryError::InvalidRange));
    }

    #[test]
    fn reserved_memory_children_are_removed_from_the_policy() {
        let mut tree = DeviceTree::new();
        tree.root
            .add_property(DeviceTreeProperty::new("#address-cells", 2u32.to_be_bytes()).unwrap());
        tree.root
            .add_property(DeviceTreeProperty::new("#size-cells", 2u32.to_be_bytes()).unwrap());
        tree.root.add_child(
            DeviceTreeNode::builder("memory@80000000")
                .unwrap()
                .property(DeviceTreeProperty::new("device_type", b"memory\0".to_vec()).unwrap())
                .property(DeviceTreeProperty::new("reg", reg(0x8000_0000, 0x20_000)).unwrap())
                .build(),
        );
        tree.root.add_child(
            DeviceTreeNode::builder("reserved-memory")
                .unwrap()
                .property(DeviceTreeProperty::new("#address-cells", 2u32.to_be_bytes()).unwrap())
                .property(DeviceTreeProperty::new("#size-cells", 2u32.to_be_bytes()).unwrap())
                .property(DeviceTreeProperty::new("ranges", Vec::new()).unwrap())
                .child(
                    DeviceTreeNode::builder("buffer@80010000")
                        .unwrap()
                        .property(DeviceTreeProperty::new("reg", reg(0x8001_0000, 0x1000)).unwrap())
                        .build(),
                )
                .build(),
        );

        let boot = BootInfo::from_test_dtb(tree.to_dtb());
        let memory = SupervisorMemory::from_boot(&boot).unwrap();
        assert!(memory.reader(0x8000_0000, 0, 0x1_0000).is_ok());
        assert_eq!(
            memory.reader(0x8001_0000, 0, 1).err(),
            Some(MemoryError::InvalidRange)
        );
        assert!(memory.reader(0x8001_1000, 0, 0xf000).is_ok());
    }

    fn reg(start: u32, size: u32) -> Vec<u8> {
        [0, start, 0, size]
            .into_iter()
            .flat_map(u32::to_be_bytes)
            .collect()
    }
}
