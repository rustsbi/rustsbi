//! Bounded volatile access to one permanently claimed ordinary MMIO range.

use core::marker::PhantomData;
use core::mem::{align_of, size_of};
use core::ops::Range;

use crate::boot::{BootInfo, MachineRangeError};

mod sealed {
    pub trait Sealed {}
}

/// A scalar register value supported by [`IoMem`].
///
/// This trait is sealed: MMIO does not accept arbitrary Rust layouts.
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
                    // SAFETY: the caller validated the complete aligned access
                    // inside an exclusively claimed MMIO range.
                    unsafe { (address as *const Self).read_volatile() }
                }

                unsafe fn write(address: usize, value: Self) {
                    // SAFETY: same range and alignment proof as `read`.
                    unsafe { (address as *mut Self).write_volatile(value) }
                }
            }
        )+
    };
}

impl_io_value!(u8, u16, u32);

/// Failure while claiming or accessing an ordinary MMIO range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IoMemError {
    /// The range is empty, wraps, or violates machine-range alignment.
    InvalidRange,
    /// The range overlaps a resource already owned by machine firmware.
    AlreadyClaimed,
    /// The requested region or scalar access lies outside the claimed range.
    OutOfBounds,
    /// The scalar access is not naturally aligned.
    Misaligned,
}

/// Exclusive ownership of one ordinary physical MMIO range.
///
/// The capability is deliberately neither `Clone` nor `Copy` and exposes no
/// physical pointer. Dropping it does not release the permanent machine claim.
pub struct IoMem {
    range: Range<usize>,
    _not_sendable_by_layout_accident: PhantomData<fn() -> ()>,
}

impl IoMem {
    /// Permanently claims an ordinary MMIO range during boot.
    ///
    /// This constructor is crate-private until the external-driver child
    /// installs the public classified acquisition registry.
    pub(crate) fn acquire(boot: &mut BootInfo, range: Range<usize>) -> Result<Self, IoMemError> {
        boot.claim_machine_range(range.clone())
            .map_err(|error| match error {
                MachineRangeError::Invalid => IoMemError::InvalidRange,
                MachineRangeError::AlreadyClaimed => IoMemError::AlreadyClaimed,
            })?;
        Ok(Self {
            range,
            _not_sendable_by_layout_accident: PhantomData,
        })
    }

    /// Returns the size in bytes of the claimed range.
    pub fn len(&self) -> usize {
        self.range.end - self.range.start
    }

    /// Returns whether the claimed range is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Creates a borrowed view over a relative subrange.
    pub fn region(&self, range: Range<usize>) -> Result<IoMemRegion<'_>, IoMemError> {
        let absolute = checked_subrange(&self.range, range)?;
        Ok(IoMemRegion {
            range: absolute,
            owner: PhantomData,
        })
    }

    /// Performs one naturally aligned volatile scalar read.
    pub fn read_once<T: IoValue>(&self, offset: usize) -> Result<T, IoMemError> {
        read_once(&self.range, offset)
    }

    /// Performs one naturally aligned volatile scalar write.
    pub fn write_once<T: IoValue>(&self, offset: usize, value: T) -> Result<(), IoMemError> {
        write_once(&self.range, offset, value)
    }
}

/// A borrowed, bounded view into an [`IoMem`] capability.
pub struct IoMemRegion<'io> {
    range: Range<usize>,
    owner: PhantomData<&'io IoMem>,
}

impl IoMemRegion<'_> {
    /// Returns the view size in bytes.
    pub fn len(&self) -> usize {
        self.range.end - self.range.start
    }

    /// Returns whether the view is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Performs one naturally aligned volatile scalar read inside this view.
    pub fn read_once<T: IoValue>(&self, offset: usize) -> Result<T, IoMemError> {
        read_once(&self.range, offset)
    }

    /// Performs one naturally aligned volatile scalar write inside this view.
    pub fn write_once<T: IoValue>(&self, offset: usize, value: T) -> Result<(), IoMemError> {
        write_once(&self.range, offset, value)
    }
}

fn checked_subrange(
    owner: &Range<usize>,
    relative: Range<usize>,
) -> Result<Range<usize>, IoMemError> {
    if relative.start > relative.end {
        return Err(IoMemError::OutOfBounds);
    }
    let start = owner
        .start
        .checked_add(relative.start)
        .ok_or(IoMemError::OutOfBounds)?;
    let end = owner
        .start
        .checked_add(relative.end)
        .ok_or(IoMemError::OutOfBounds)?;
    (start <= end && end <= owner.end)
        .then_some(start..end)
        .ok_or(IoMemError::OutOfBounds)
}

fn checked_address<T: IoValue>(range: &Range<usize>, offset: usize) -> Result<usize, IoMemError> {
    let address = range
        .start
        .checked_add(offset)
        .ok_or(IoMemError::OutOfBounds)?;
    let end = address
        .checked_add(size_of::<T>())
        .ok_or(IoMemError::OutOfBounds)?;
    if end > range.end {
        return Err(IoMemError::OutOfBounds);
    }
    if !address.is_multiple_of(align_of::<T>()) {
        return Err(IoMemError::Misaligned);
    }
    Ok(address)
}

fn read_once<T: IoValue>(range: &Range<usize>, offset: usize) -> Result<T, IoMemError> {
    let address = checked_address::<T>(range, offset)?;
    // SAFETY: `checked_address` proved width, bounds and natural alignment.
    Ok(unsafe { T::read(address) })
}

fn write_once<T: IoValue>(range: &Range<usize>, offset: usize, value: T) -> Result<(), IoMemError> {
    let address = checked_address::<T>(range, offset)?;
    // SAFETY: `checked_address` proved width, bounds and natural alignment.
    unsafe { T::write(address, value) };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regions_are_relative_and_bounded() {
        let io = IoMem {
            range: 0x1000..0x1100,
            _not_sendable_by_layout_accident: PhantomData,
        };
        let region = io.region(0x20..0x40).unwrap();
        assert_eq!(region.range, 0x1020..0x1040);
        assert_eq!(region.len(), 0x20);
        assert_eq!(io.region(0x80..0x101).err(), Some(IoMemError::OutOfBounds));
    }

    #[test]
    fn scalar_validation_checks_width_and_alignment_without_accessing_mmio() {
        let range = 0x1000..0x1010;
        assert_eq!(checked_address::<u32>(&range, 4), Ok(0x1004));
        assert_eq!(
            checked_address::<u32>(&range, 2),
            Err(IoMemError::Misaligned)
        );
        assert_eq!(
            checked_address::<u32>(&range, 14),
            Err(IoMemError::OutOfBounds)
        );
    }
}
