//! Bounded access to supervisor-accessible RAM.

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU8, Ordering};

use super::{PhysAddr, PhysAddrRange};
use crate::{Error, Result};

/// Bounded access to registered supervisor RAM.
///
/// This represents physical RAM that firmware may access on behalf of
/// supervisor software, not the supervisor's virtual address space.
/// The handle may cover multiple RAM banks.
/// The range description is immutable, so validation does not require a
/// global lock. The bytes lie outside firmware-owned Rust allocations and are
/// accessed as atomic bytes so separate harts can service overlapping SBI
/// buffers without a global lock.
/// The handle cannot be cloned or used to manufacture raw slices.
/// Access faults are not recovered because registered RAM is required to
/// remain accessible.
pub struct SupervisorMemory {
    regions: Vec<PhysAddrRange>,
}

impl SupervisorMemory {
    pub(crate) fn new(regions: Vec<PhysAddrRange>) -> Self {
        Self { regions }
    }

    /// Checks that `range` lies in one available RAM region.
    pub fn check_range(&self, range: PhysAddrRange) -> Result<()> {
        if self
            .regions
            .iter()
            .copied()
            .any(|available| available.contains(range))
        {
            Ok(())
        } else {
            Err(Error::InvalidArgs)
        }
    }

    fn check_access(&self, start: PhysAddr, len: usize) -> Result<()> {
        if len == 0 {
            return Ok(());
        }
        self.check_range(PhysAddrRange::from_start_len(start, len)?)
    }

    /// Copies `output.len()` bytes from registered RAM into `output`.
    ///
    /// The complete span is checked before any byte is read. An empty output
    /// succeeds without inspecting `start`.
    ///
    pub fn read(&self, start: PhysAddr, output: &mut [u8]) -> Result<()> {
        self.check_access(start, output.len())?;
        for (offset, byte) in output.iter_mut().enumerate() {
            // SAFETY: `check_access` confined this byte to registered RAM.
            // `AtomicU8` has byte alignment and preserves every bit pattern.
            let source = unsafe { &*(start.as_usize() as *const AtomicU8).add(offset) };
            *byte = source.load(Ordering::Relaxed);
        }
        Ok(())
    }

    /// Copies `input` into registered RAM starting at `start`.
    ///
    /// The complete span is checked before any byte is written. An empty input
    /// succeeds without inspecting `start`.
    ///
    pub fn write(&self, start: PhysAddr, input: &[u8]) -> Result<()> {
        self.check_access(start, input.len())?;
        for (offset, byte) in input.iter().copied().enumerate() {
            // SAFETY: `check_access` confined this byte to registered RAM.
            // `AtomicU8` has byte alignment and accepts every `u8` value.
            let destination = unsafe { &*(start.as_usize() as *const AtomicU8).add(offset) };
            destination.store(byte, Ordering::Relaxed);
        }
        Ok(())
    }

    /// Fills a registered RAM range with zero bytes.
    ///
    /// The complete range is checked before any byte is changed.
    ///
    pub fn fill_zeros(&self, start: PhysAddr, len: usize) -> Result<()> {
        self.check_access(start, len)?;
        if len == 0 {
            return Ok(());
        }
        for offset in 0..len {
            // SAFETY: `check_access` confined this byte to registered RAM.
            // `AtomicU8` has byte alignment and accepts zero.
            let destination = unsafe { &*(start.as_usize() as *const AtomicU8).add(offset) };
            destination.store(0, Ordering::Relaxed);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    #[test]
    fn reads_writes_and_clears_complete_bounded_slices() {
        let mut backing = [1u8, 2, 3, 4];
        let range = PhysAddrRange::from_start_len(
            PhysAddr::new(backing.as_mut_ptr() as usize),
            backing.len(),
        )
        .unwrap();
        let memory = SupervisorMemory::new(vec![range]);

        let mut output = [0; 4];
        assert_eq!(memory.read(range.start(), &mut output), Ok(()));
        assert_eq!(output, backing);

        assert_eq!(memory.write(range.start(), &[9, 8, 7, 6]), Ok(()));
        assert_eq!(backing, [9, 8, 7, 6]);
        assert_eq!(memory.fill_zeros(range.start(), backing.len()), Ok(()));
        assert_eq!(backing, [0; 4]);
        assert_eq!(
            memory.read(range.end(), &mut [0; 1]),
            Err(Error::InvalidArgs)
        );
    }

    #[test]
    fn zero_length_access_does_not_require_an_addressable_byte() {
        let memory = SupervisorMemory::new(vec![
            PhysAddrRange::from_start_len(PhysAddr::new(1), 1).unwrap(),
        ]);
        assert_eq!(memory.read(PhysAddr::new(usize::MAX), &mut []), Ok(()));
        assert_eq!(memory.write(PhysAddr::new(usize::MAX), &[]), Ok(()));
    }

    #[test]
    fn overflowing_access_is_rejected_before_memory_is_touched() {
        let memory = SupervisorMemory::new(vec![
            PhysAddrRange::from_start_len(PhysAddr::new(1), 1).unwrap(),
        ]);
        let mut output = [0xaa; 2];

        assert_eq!(
            memory.read(PhysAddr::new(usize::MAX), &mut output),
            Err(Error::Overflow)
        );
        assert_eq!(output, [0xaa; 2]);
    }
}
