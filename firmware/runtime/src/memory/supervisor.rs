//! Bounded access to supervisor-accessible RAM.

use alloc::vec::Vec;

use super::{PhysAddr, PhysAddrRange};
use crate::{Error, Result};

/// Reads complete byte sequences from registered memory.
pub trait ReadMemory {
    /// Copies `output.len()` bytes starting at `start` into `output`.
    ///
    /// The operation either initializes the complete output slice or returns
    /// an error. An empty output succeeds without inspecting `start`.
    /// A non-empty span must fit within one registered RAM range.
    fn read(&self, start: PhysAddr, output: &mut [u8]) -> Result<()>;
}

/// Writes complete byte sequences to registered memory.
pub trait WriteMemory {
    /// Copies all of `input` to memory starting at `start`.
    ///
    /// The operation either writes the complete input slice or returns an
    /// error. An empty input succeeds without inspecting `start`.
    /// A non-empty span must fit within one registered RAM range.
    fn write(&mut self, start: PhysAddr, input: &[u8]) -> Result<()>;
}

/// Bounded access to registered supervisor RAM.
///
/// The handle may cover multiple RAM banks.
/// It can be shared for concurrent reads, while writes require an exclusive
/// borrow. It cannot be cloned or used to manufacture raw slices.
/// Access faults are not recovered because registered RAM is required to
/// remain accessible.
pub struct SupervisorMemory {
    regions: Vec<PhysAddrRange>,
}

impl SupervisorMemory {
    pub(crate) fn new(regions: Vec<PhysAddrRange>) -> Self {
        Self { regions }
    }

    fn check_access(&self, start: PhysAddr, len: usize) -> Result<()> {
        if len == 0 {
            return Ok(());
        }
        let range = PhysAddrRange::from_start_len(start, len)?;
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
}

impl ReadMemory for SupervisorMemory {
    fn read(&self, start: PhysAddr, output: &mut [u8]) -> Result<()> {
        self.check_access(start, output.len())?;
        for (offset, byte) in output.iter_mut().enumerate() {
            // SAFETY:
            // 1. `check_access` confines the complete span to one registered
            //    RAM range that permits volatile byte access.
            // 2. `u8` has no alignment restriction and every bit pattern is
            //    valid.
            *byte = unsafe { (start.as_usize() as *const u8).add(offset).read_volatile() };
        }
        Ok(())
    }
}

impl WriteMemory for SupervisorMemory {
    fn write(&mut self, start: PhysAddr, input: &[u8]) -> Result<()> {
        self.check_access(start, input.len())?;
        for (offset, byte) in input.iter().copied().enumerate() {
            // SAFETY:
            // 1. `check_access` confines the complete span to one registered
            //    RAM range that permits volatile byte access.
            // 2. `u8` has no alignment restriction.
            unsafe {
                (start.as_usize() as *mut u8)
                    .add(offset)
                    .write_volatile(byte)
            };
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    #[test]
    fn reads_and_writes_complete_bounded_slices() {
        let mut backing = [1u8, 2, 3, 4];
        let range = PhysAddrRange::from_start_len(
            PhysAddr::new(backing.as_mut_ptr() as usize),
            backing.len(),
        )
        .unwrap();
        let mut memory = SupervisorMemory::new(vec![range]);

        let mut output = [0; 4];
        assert_eq!(memory.read(range.start(), &mut output), Ok(()));
        assert_eq!(output, backing);

        assert_eq!(memory.write(range.start(), &[9, 8, 7, 6]), Ok(()));
        assert_eq!(backing, [9, 8, 7, 6]);
        assert_eq!(
            memory.read(range.end(), &mut [0; 1]),
            Err(Error::InvalidArgs)
        );
    }

    #[test]
    fn zero_length_access_does_not_require_an_addressable_byte() {
        let mut memory = SupervisorMemory::new(vec![
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

    #[test]
    fn supervisor_memory_is_shareable_by_reference() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<SupervisorMemory>();
    }
}
