//! Access to memory-mapped device registers.

use core::mem::{align_of, size_of};

use super::PhysAddrRange;
use crate::{Error, Result};

mod sealed {
    pub trait Sealed {}
}

/// A fixed-width integer supported by [`MmioRegion`].
///
/// This trait is sealed and implemented for `u8`, `u16`, and `u32`, as well as
/// `u64` on 64-bit targets.
pub trait MmioValue: sealed::Sealed + Copy {}

macro_rules! impl_mmio_value {
    ($($value:ty),* $(,)?) => {
        $(
            impl sealed::Sealed for $value {}
            impl MmioValue for $value {}
        )*
    };
}

impl_mmio_value!(u8, u16, u32);

// A volatile `u64` may compile into multiple device accesses on a 32-bit
// target, so it is not offered as one fixed-width MMIO operation there.
#[cfg(target_pointer_width = "64")]
impl_mmio_value!(u64);

/// A bounded MMIO register window.
///
/// Accesses are volatile and use native byte order. Hardware access faults are
/// not recovered.
pub struct MmioRegion {
    range: PhysAddrRange,
}

impl MmioRegion {
    pub(crate) const fn new(range: PhysAddrRange) -> Self {
        Self { range }
    }

    /// Reads a value at byte offset `offset`.
    pub fn read<T: MmioValue>(&self, offset: usize) -> Result<T> {
        let address = self.checked_address::<T>(offset)?;
        // SAFETY: `MmioValue` is sealed to integers, and `checked_address`
        // checked the
        // complete access and its alignment within this registered window.
        Ok(unsafe { (address as *const T).read_volatile() })
    }

    /// Writes a value at byte offset `offset`.
    pub fn write<T: MmioValue>(&self, offset: usize, value: T) -> Result<()> {
        let address = self.checked_address::<T>(offset)?;
        // SAFETY: `MmioValue` is sealed to integers, and `checked_address`
        // checked the
        // complete access and its alignment within this registered window.
        unsafe { (address as *mut T).write_volatile(value) };
        Ok(())
    }

    fn checked_address<T: MmioValue>(&self, offset: usize) -> Result<usize> {
        let access = self.range.subrange(offset, size_of::<T>())?;
        let address = access.start().as_usize();
        if !address.is_multiple_of(align_of::<T>()) {
            return Err(Error::InvalidArgs);
        }
        Ok(address)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::PhysAddr;

    #[repr(align(8))]
    struct Aligned([u8; 16]);

    #[test]
    fn accesses_check_width_alignment_and_bounds() {
        let mut backing = Aligned([0; 16]);
        let range = PhysAddrRange::from_start_len(
            PhysAddr::new(backing.0.as_mut_ptr() as usize),
            backing.0.len(),
        )
        .unwrap();
        let registers = MmioRegion::new(range);

        assert_eq!(registers.write(4, 0x1234_5678u32), Ok(()));
        assert_eq!(registers.read::<u32>(4), Ok(0x1234_5678));
        assert_eq!(registers.read::<u32>(1), Err(Error::InvalidArgs));
        assert_eq!(registers.read::<u32>(14), Err(Error::InvalidArgs));
        assert_eq!(registers.write(1, 0u32), Err(Error::InvalidArgs));
        assert_eq!(registers.write(1, 0x5au8), Ok(()));
        assert_eq!(registers.read::<u8>(1), Ok(0x5a));
    }
}
