//! Exclusive ordinary-MMIO ownership and bounded volatile register access.

use core::marker::PhantomData;
use core::mem::{align_of, size_of};
use core::ops::Range;

use crate::boot::BootInfo;

mod sealed {
    pub trait Sealed {}
}

/// A scalar register value supported by [`IoMem`].
pub trait IoValue: sealed::Sealed + Copy {
    #[doc(hidden)]
    unsafe fn read(address: usize) -> Self;

    #[doc(hidden)]
    unsafe fn write(address: usize, value: Self);
}

macro_rules! impl_io_value {
    ($($type:ty),+ $(,)?) => {
        $(
            impl sealed::Sealed for $type {}

            impl IoValue for $type {
                unsafe fn read(address: usize) -> Self {
                    // SAFETY: the caller proved that this aligned scalar lies
                    // inside the exclusively owned MMIO range.
                    unsafe { (address as *const Self).read_volatile() }
                }

                unsafe fn write(address: usize, value: Self) {
                    // SAFETY: same ownership, bounds, and alignment proof as
                    // the matching read operation.
                    unsafe { (address as *mut Self).write_volatile(value) }
                }
            }
        )+
    };
}

impl_io_value!(u8, u16, u32);

/// A failed bounded MMIO register operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IoMemError {
    /// The requested view or access lies outside the owned register window.
    OutOfBounds,
    /// The scalar access does not meet its natural alignment requirement.
    Misaligned,
}

/// Exclusive ownership of one ordinary physical MMIO range.
///
/// Firmware selects the device from its owned device tree, then acquires its
/// exact register range from the single boot range ledger.  Machine therefore
/// owns neither a device tree parser nor a global device registry.
pub struct IoMem {
    range: Range<usize>,
    _not_sendable_by_layout_accident: PhantomData<fn() -> ()>,
}

impl IoMem {
    /// Claims one aligned, non-overlapping ordinary device window for the
    /// lifetime of this boot.
    pub fn acquire(boot: &mut BootInfo, range: Range<usize>) -> Option<Self> {
        boot.claim_machine_range(range.clone()).then_some(())?;
        Some(Self {
            range,
            _not_sendable_by_layout_accident: PhantomData,
        })
    }

    /// Returns the size of the owned register window in bytes.
    pub fn len(&self) -> usize {
        self.range.end - self.range.start
    }

    /// Returns whether the owned register window has no bytes.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the exact physical window owned by this capability.
    #[doc(hidden)]
    pub(crate) fn range(&self) -> &Range<usize> {
        &self.range
    }

    /// Checks whether a complete physical scalar lies inside this capability.
    #[doc(hidden)]
    pub(crate) fn covers(&self, address: usize, bytes: usize) -> bool {
        address
            .checked_add(bytes)
            .is_some_and(|end| address >= self.range.start && end <= self.range.end)
    }

    /// Creates a bounded view relative to this window.
    pub fn region(&self, range: Range<usize>) -> Result<IoMemRegion<'_>, IoMemError> {
        let range = self.subrange(range)?;
        Ok(IoMemRegion {
            range,
            owner: PhantomData,
        })
    }

    /// Performs one naturally aligned volatile scalar read.
    pub fn read_once<T: IoValue>(&self, offset: usize) -> Result<T, IoMemError> {
        let address = self.address::<T>(offset)?;
        // SAFETY: `address` proved the complete scalar lies inside this
        // exclusively owned register window with natural alignment.
        Ok(unsafe { T::read(address) })
    }

    /// Performs one naturally aligned volatile scalar write.
    pub fn write_once<T: IoValue>(&self, offset: usize, value: T) -> Result<(), IoMemError> {
        let address = self.address::<T>(offset)?;
        // SAFETY: `address` proves the same ownership, bounds, and alignment
        // conditions as `read_once`.
        unsafe { T::write(address, value) };
        Ok(())
    }

    /// Validates one scalar register offset without touching the device.
    pub fn validate<T: IoValue>(&self, offset: usize) -> Result<(), IoMemError> {
        self.address::<T>(offset).map(|_| ())
    }

    fn subrange(&self, range: Range<usize>) -> Result<Range<usize>, IoMemError> {
        if range.start > range.end {
            return Err(IoMemError::OutOfBounds);
        }
        let start = self
            .range
            .start
            .checked_add(range.start)
            .ok_or(IoMemError::OutOfBounds)?;
        let end = self
            .range
            .start
            .checked_add(range.end)
            .ok_or(IoMemError::OutOfBounds)?;
        (end <= self.range.end)
            .then_some(start..end)
            .ok_or(IoMemError::OutOfBounds)
    }

    fn address<T: IoValue>(&self, offset: usize) -> Result<usize, IoMemError> {
        let address = self
            .range
            .start
            .checked_add(offset)
            .ok_or(IoMemError::OutOfBounds)?;
        let end = address
            .checked_add(size_of::<T>())
            .ok_or(IoMemError::OutOfBounds)?;
        if end > self.range.end {
            return Err(IoMemError::OutOfBounds);
        }
        if !address.is_multiple_of(align_of::<T>()) {
            return Err(IoMemError::Misaligned);
        }
        Ok(address)
    }
}

impl riscv_aia::aplic::VolatileAccess for IoMem {
    type Error = IoMemError;

    fn len(&self) -> usize {
        Self::len(self)
    }

    fn read_u32(&self, offset: usize) -> Result<u32, Self::Error> {
        self.read_once(offset)
    }

    fn write_u32(&self, offset: usize, value: u32) -> Result<(), Self::Error> {
        self.write_once(offset, value)
    }

    fn fence_iorw(&self) {
        io_fence();
    }
}

/// A borrowed, bounded view into an [`IoMem`] window.
pub struct IoMemRegion<'io> {
    range: Range<usize>,
    owner: PhantomData<&'io IoMem>,
}

impl IoMemRegion<'_> {
    /// Returns the size of this view in bytes.
    pub fn len(&self) -> usize {
        self.range.end - self.range.start
    }

    /// Returns whether this view has no bytes.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Performs one naturally aligned volatile scalar read inside this view.
    pub fn read_once<T: IoValue>(&self, offset: usize) -> Result<T, IoMemError> {
        let io = IoMem {
            range: self.range.clone(),
            _not_sendable_by_layout_accident: PhantomData,
        };
        io.read_once(offset)
    }

    /// Performs one naturally aligned volatile scalar write inside this view.
    pub fn write_once<T: IoValue>(&self, offset: usize, value: T) -> Result<(), IoMemError> {
        let io = IoMem {
            range: self.range.clone(),
            _not_sendable_by_layout_accident: PhantomData,
        };
        io.write_once(offset, value)
    }
}

/// Orders ordinary MMIO accesses against other RISC-V I/O and memory accesses.
pub fn io_fence() {
    // SAFETY: this fixed ordering operation has no address or register input.
    unsafe { core::arch::asm!("fence iorw, iorw", options(nostack)) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn views_and_scalar_accesses_stay_within_the_owned_window() {
        let io = IoMem {
            range: 0x1000..0x1100,
            _not_sendable_by_layout_accident: PhantomData,
        };
        assert!(matches!(
            io.region(0x80..0x101),
            Err(IoMemError::OutOfBounds)
        ));
        assert_eq!(io.validate::<u32>(0x101), Err(IoMemError::OutOfBounds));
        assert_eq!(io.validate::<u32>(1), Err(IoMemError::Misaligned));
    }
}
