//! Bounded access to supervisor-accessible RAM.

use alloc::vec::Vec;
use spin::Mutex;

use super::{PhysAddr, PhysAddrRange};
use crate::{Error, Result};

/// Serializes supervisor-memory byte transfers across every handle.
///
/// [`PlatformDescription::memory_resources()`](crate::PlatformDescription::memory_resources)
/// derives a fresh handle on each call, so per-handle locks would not
/// exclude transfers issued through different handles over the same RAM.
/// One static lock keeps every transfer mutually exclusive.
static TRANSFER_LOCK: Mutex<()> = Mutex::new(());

/// Bounded access to registered supervisor RAM.
///
/// This represents physical RAM that firmware may access on behalf of
/// supervisor software, not the supervisor's virtual address space.
/// The handle may cover multiple RAM banks.
/// The range description is immutable, so validation does not require a
/// global lock. The bytes lie outside firmware-owned Rust allocations, so
/// they are accessed as volatile bytes; one shared transfer lock serializes
/// transfers from separate firmware harts, keeping overlapping SBI buffers
/// race-free without per-byte atomics.
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
    /// succeeds without inspecting `start`. Transfers serialize on the
    /// transfer lock, so a transfer observes either the state before or the
    /// state after another firmware hart's whole transfer.
    ///
    pub fn read(&self, start: PhysAddr, output: &mut [u8]) -> Result<()> {
        self.check_access(start, output.len())?;
        if output.is_empty() {
            return Ok(());
        }
        let _transfer = TRANSFER_LOCK.lock();
        for (offset, byte) in output.iter_mut().enumerate() {
            // SAFETY: Obligation: `read_volatile` requires a pointer valid for
            // reads and properly aligned.
            // Artifact facts: `check_access` passed the whole span through
            // `PhysAddrRange::from_start_len`, whose checked `start + len`
            // end confines `start + offset` (offset < len) to a registered
            // region; the address is recomputed as plain `usize` arithmetic
            // instead of pointer `add`, which would require an allocation.
            // Semantic premises:
            // - The volatile-load exemption covers memory outside every Rust
            //   allocation; registered supervisor RAM is deliberately excluded
            //   from firmware-owned storage (`reserved ∪ firmware image`),
            //   so no live firmware allocation claims these bytes.
            // - `u8` requires alignment 1, which every address satisfies, and
            //   preserves every bit pattern.
            // - The transfer lock excludes other firmware harts from
            //   concurrent access to the same bytes, so this volatile load
            //   cannot race within the firmware's Rust abstract machine.
            // Result: the load returns the current byte value without UB.
            *byte = unsafe { ((start.as_usize() + offset) as *const u8).read_volatile() };
        }
        Ok(())
    }

    /// Copies `input` into registered RAM starting at `start`.
    ///
    /// The complete span is checked before any byte is written. An empty input
    /// succeeds without inspecting `start`. Transfers serialize on the
    /// transfer lock, so a transfer observes either the state before or the
    /// state after another firmware hart's whole transfer.
    ///
    pub fn write(&self, start: PhysAddr, input: &[u8]) -> Result<()> {
        self.check_access(start, input.len())?;
        if input.is_empty() {
            return Ok(());
        }
        let _transfer = TRANSFER_LOCK.lock();
        for (offset, byte) in input.iter().copied().enumerate() {
            // SAFETY: Obligation: `write_volatile` requires a pointer valid
            // for writes and properly aligned.
            // Artifact facts: `check_access` passed the whole span through
            // `PhysAddrRange::from_start_len`, whose checked `start + len`
            // end confines `start + offset` (offset < len) to a registered
            // region; the address is recomputed as plain `usize` arithmetic
            // instead of pointer `add`, which would require an allocation.
            // Semantic premises:
            // - The volatile-store exemption covers memory outside every Rust
            //   allocation; registered supervisor RAM is deliberately excluded
            //   from firmware-owned storage (`reserved ∪ firmware image`),
            //   so no live firmware allocation claims these bytes.
            // - `u8` requires alignment 1, which every address satisfies, and
            //   accepts every value.
            // - The transfer lock excludes other firmware harts from
            //   concurrent access to the same bytes, so this volatile store
            //   cannot race within the firmware's Rust abstract machine.
            // Result: the store writes `byte` without UB and leaves the
            // registry's immutable region description unchanged.
            unsafe { ((start.as_usize() + offset) as *mut u8).write_volatile(byte) };
        }
        Ok(())
    }

    /// Fills a registered RAM range with zero bytes.
    ///
    /// The complete range is checked before any byte is changed. Transfers
    /// serialize on the transfer lock, so a transfer observes either the
    /// state before or the state after another firmware hart's whole
    /// transfer.
    ///
    pub fn fill_zeros(&self, start: PhysAddr, len: usize) -> Result<()> {
        self.check_access(start, len)?;
        if len == 0 {
            return Ok(());
        }
        let _transfer = TRANSFER_LOCK.lock();
        for offset in 0..len {
            // SAFETY: Obligation: `write_volatile` requires a pointer valid
            // for writes and properly aligned.
            // Artifact facts: `check_access` passed the whole span through
            // `PhysAddrRange::from_start_len`, whose checked `start + len`
            // end confines `start + offset` (offset < len) to a registered
            // region; the address is recomputed as plain `usize` arithmetic
            // instead of pointer `add`, which would require an allocation.
            // Semantic premises:
            // - The volatile-store exemption covers memory outside every Rust
            //   allocation; registered supervisor RAM is deliberately excluded
            //   from firmware-owned storage (`reserved ∪ firmware image`),
            //   so no live firmware allocation claims these bytes.
            // - `u8` requires alignment 1, which every address satisfies, and
            //   accepts zero.
            // - The transfer lock excludes other firmware harts from
            //   concurrent access to the same bytes, so this volatile store
            //   cannot race within the firmware's Rust abstract machine.
            // Result: the store writes zero without UB.
            unsafe { ((start.as_usize() + offset) as *mut u8).write_volatile(0) };
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    extern crate std;
    use std::thread;

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

    /// Exercises overlapping transfers from concurrent callers through the
    /// transfer lock. The soundness of the locked path is established by the
    /// SAFETY proofs, not by this test: a data race cannot be observed
    /// reliably from the outside.
    #[test]
    fn concurrent_overlapping_transfers_serialize_on_the_transfer_lock() {
        let mut backing = [0u8; 64];
        let range = PhysAddrRange::from_start_len(
            PhysAddr::new(backing.as_mut_ptr() as usize),
            backing.len(),
        )
        .unwrap();
        let memory = SupervisorMemory::new(vec![range]);
        let memory = &memory;

        thread::scope(|scope| {
            scope.spawn(|| {
                for _ in 0..128 {
                    memory.write(range.start(), &[0xff; 64]).unwrap();
                }
            });
            scope.spawn(|| {
                for _ in 0..128 {
                    memory.write(range.start(), &[0x00; 64]).unwrap();
                }
            });
            scope.spawn(|| {
                let mut buffer = [0x5a; 64];
                for _ in 0..128 {
                    memory.read(range.start(), &mut buffer).unwrap();
                    // Transfers are mutually exclusive, so a read observes
                    // one whole transfer, never an interleaving.
                    assert!(
                        buffer.iter().all(|byte| matches!(*byte, 0 | 0xff)),
                        "transfer interleaved: {buffer:?}"
                    );
                }
            });
        });

        assert!(backing.iter().all(|byte| matches!(*byte, 0 | 0xff)));
    }
}
