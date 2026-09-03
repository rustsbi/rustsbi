//! Typed access to memory-mapped device registers.

use core::marker::PhantomData;
use core::mem::{align_of, size_of};

use super::PhysAddrRange;
use crate::{Error, Result};

mod sealed {
    pub trait Sealed {}
}

/// A fixed-width integer supported by [`MmioRegion`].
///
/// This trait is sealed and implemented for `u8`, `u16`, `u32`, and `u64`.
pub trait MmioValue: sealed::Sealed + Copy {}

macro_rules! impl_mmio_value {
    ($($value:ty),* $(,)?) => {
        $(
            impl sealed::Sealed for $value {}
            impl MmioValue for $value {}
        )*
    };
}

impl_mmio_value!(u8, u16, u32, u64);

/// A bounded MMIO window accessed in units of `T`.
///
/// `T` is fixed when the window is acquired, and values use native byte order.
/// Mixed-width register blocks use separate, non-overlapping windows.
pub struct MmioRegion<T: MmioValue> {
    range: PhysAddrRange,
    value: PhantomData<T>,
}

impl<T: MmioValue> MmioRegion<T> {
    pub(crate) const fn new(range: PhysAddrRange) -> Self {
        Self {
            range,
            value: PhantomData,
        }
    }

    /// Creates another handle to the same MMIO window.
    #[inline]
    pub fn share(&self) -> Self {
        Self::new(self.range)
    }

    /// Returns a handle to the selected non-empty subrange.
    ///
    /// The range must lie entirely within this MMIO window.
    pub fn subregion(&self, offset: usize, len: usize) -> Result<Self> {
        self.range.subrange(offset, len).map(Self::new)
    }

    /// Reads one `T` at byte offset `offset`.
    ///
    /// Returns [`Error::InvalidArgs`] if the access is out of bounds or
    /// misaligned. Hardware access faults are not recovered.
    pub fn read(&self, offset: usize) -> Result<T> {
        let address = self.access_address(offset)?;
        // SAFETY:
        // 1. `MmioValue` is sealed to integer types with no invalid values.
        // 2. `access_address` checked the complete access and its alignment.
        // 3. Registration keeps the MMIO window accessible.
        Ok(unsafe { (address as *const T).read_volatile() })
    }

    /// Writes one `T` at byte offset `offset`.
    ///
    /// Returns [`Error::InvalidArgs`] if the access is out of bounds or
    /// misaligned. Hardware access faults are not recovered.
    pub fn write(&self, offset: usize, value: T) -> Result<()> {
        let address = self.access_address(offset)?;
        // SAFETY:
        // 1. `MmioValue` is sealed to integer types supported by this method.
        // 2. `access_address` checked the complete access and its alignment.
        // 3. Registration keeps the MMIO window accessible.
        unsafe { (address as *mut T).write_volatile(value) };
        Ok(())
    }

    /// Resolves one aligned, in-bounds `T` at `offset`.
    fn access_address(&self, offset: usize) -> Result<usize> {
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
    fn access_width_is_fixed_by_the_handle_type() {
        let mut backing = Aligned([0; 16]);
        let range = PhysAddrRange::from_start_len(
            PhysAddr::new(backing.0.as_mut_ptr() as usize),
            backing.0.len(),
        )
        .unwrap();
        let words = MmioRegion::<u32>::new(range);

        assert_eq!(words.write(4, 0x1234_5678), Ok(()));
        assert_eq!(words.read(4), Ok(0x1234_5678));
        assert_eq!(words.read(1), Err(Error::InvalidArgs));
        assert_eq!(words.read(14), Err(Error::InvalidArgs));
        assert_eq!(words.write(1, 0), Err(Error::InvalidArgs));
        assert_eq!(words.subregion(usize::MAX, 1).err(), Some(Error::Overflow));

        let bytes = MmioRegion::<u8>::new(range);
        assert_eq!(bytes.write(1, 0x5a), Ok(()));
        assert_eq!(bytes.read(1), Ok(0x5a));
    }
}
