//! Checked physical access to next-stage shared memory.
//!
//! The capability does not own, allocate, retype, or borrow external frames.
//! It authorizes byte transfer by value only; no Rust reference is ever formed
//! from the supplied physical address.

use alloc::vec::Vec;
use core::ops::Range;

use sbi_spec::binary::Physical;

use crate::boot::BootInfo;
use crate::trap::probe::{ExpectedResult, load_byte, store_byte};

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
    /// Builds bounded supervisor RAM access from firmware-selected normal-RAM
    /// ranges, subtracting every range already retained by machine firmware.
    pub(crate) fn from_boot(boot: &BootInfo, memory: &[Range<usize>]) -> Result<Self, MemoryError> {
        let mut excluded = boot.machine_ranges().to_vec();
        excluded.push(crate::pmp::machine_image_range().ok_or(MemoryError::InvalidRange)?);

        let ranges = subtract(memory.to_vec(), excluded)?;
        if ranges.is_empty() {
            return Err(MemoryError::InvalidRange);
        }
        Ok(Self { ranges })
    }

    /// Validates a complete physical range for machine reads.
    pub fn reader(&self, bytes: Physical<&[u8]>) -> Result<Reader<'_>, MemoryError> {
        let range = self.validate(
            bytes.phys_addr_lo(),
            bytes.phys_addr_hi(),
            bytes.num_bytes(),
        )?;
        Ok(Reader {
            memory: self,
            cursor: range.start,
            remaining: bytes.num_bytes(),
        })
    }

    /// Validates a complete physical range for machine writes.
    pub fn writer(&self, bytes: Physical<&mut [u8]>) -> Result<Writer<'_>, MemoryError> {
        let range = self.validate(
            bytes.phys_addr_lo(),
            bytes.phys_addr_hi(),
            bytes.num_bytes(),
        )?;
        Ok(Writer {
            memory: self,
            cursor: range.start,
            remaining: bytes.num_bytes(),
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
        assert!(memory.reader(Physical::new(0x1000, 0x1000, 0)).is_ok());
        assert_eq!(
            memory.reader(Physical::new(0x1000, 0x1800, 0)).err(),
            Some(MemoryError::InvalidRange)
        );
        assert_eq!(
            memory.writer(Physical::new(1, 0x1000, 1)).err(),
            Some(MemoryError::UnsupportedRange)
        );
        assert!(
            memory
                .writer(Physical::new(0, usize::MAX, usize::MAX))
                .is_ok()
        );
    }

    #[test]
    fn cursor_rejects_a_slice_larger_than_the_validated_remainder() {
        let range = 0x1000..0x2000;
        let memory = policy(core::slice::from_ref(&range));
        let reader = memory.reader(Physical::new(2, 0x1000, 0)).unwrap();
        assert_eq!(reader.transfer_end(3), Err(MemoryError::InvalidRange));
        let writer = memory.writer(Physical::new(2, 0x1000, 0)).unwrap();
        assert_eq!(writer.transfer_end(3), Err(MemoryError::InvalidRange));
    }
}
