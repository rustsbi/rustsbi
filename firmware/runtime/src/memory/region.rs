//! Non-empty physical-address ranges.

use alloc::vec::Vec;

use super::PhysAddr;
use crate::{Error, Result};

/// A non-empty half-open range of physical addresses.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhysAddrRange {
    start: PhysAddr,
    end: PhysAddr,
}

/// A device-register range authorized by the Platform Description.
///
/// Unlike [`PhysAddrRange`], this value carries permission to request an
/// [`super::MmioRegion`] from the [`super::MemoryRegistry`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeviceRegisterRange(PhysAddrRange);

impl DeviceRegisterRange {
    pub(crate) const fn from_description(range: PhysAddrRange) -> Self {
        Self(range)
    }

    /// Returns the first address in the register range.
    #[inline]
    pub const fn start(self) -> PhysAddr {
        self.0.start()
    }

    /// Returns the exclusive end address of the register range.
    #[inline]
    pub const fn end(self) -> PhysAddr {
        self.0.end()
    }

    /// Returns whether both bounds are aligned to `alignment`.
    #[inline]
    pub const fn has_aligned_bounds(self, alignment: usize) -> bool {
        self.0.has_aligned_bounds(alignment)
    }

    /// Returns whether `range` lies entirely in this register range.
    #[inline]
    pub fn contains(self, range: PhysAddrRange) -> bool {
        self.0.contains(range)
    }

    /// Selects a non-empty subrange of these registers.
    pub fn subrange(self, offset: usize, len: usize) -> Result<Self> {
        self.0.subrange(offset, len).map(Self)
    }

    pub(super) const fn physical_range(self) -> PhysAddrRange {
        self.0
    }
}

impl PhysAddrRange {
    /// Creates the half-open range `[start, end)`.
    ///
    /// Returns [`Error::InvalidArgs`] when the range is empty or reversed.
    pub fn new(start: PhysAddr, end: PhysAddr) -> Result<Self> {
        if start >= end {
            return Err(Error::InvalidArgs);
        }
        Ok(Self { start, end })
    }

    /// Creates the half-open range `[start, start + len)`.
    ///
    /// Returns [`Error::InvalidArgs`] when `len` is zero and [`Error::Overflow`]
    /// when the end address is not representable.
    pub fn from_start_len(start: PhysAddr, len: usize) -> Result<Self> {
        if len == 0 {
            return Err(Error::InvalidArgs);
        }
        let Some(end) = start.checked_add(len) else {
            return Err(Error::Overflow);
        };
        Self::new(start, end)
    }

    /// Returns the first address in the range.
    #[inline]
    pub const fn start(self) -> PhysAddr {
        self.start
    }

    /// Returns the first address after the range.
    #[inline]
    pub const fn end(self) -> PhysAddr {
        self.end
    }

    /// Returns the size of the range in bytes.
    #[inline]
    pub const fn size(self) -> usize {
        self.end.as_usize() - self.start.as_usize()
    }

    /// Returns whether both bounds are aligned to `alignment`.
    ///
    /// This is stronger than checking only [`PhysAddr::is_aligned_to`]: it
    /// guarantees that the range consists of a whole number of aligned units.
    #[inline]
    pub(super) const fn has_aligned_bounds(self, alignment: usize) -> bool {
        self.start.is_aligned_to(alignment) && self.end.is_aligned_to(alignment)
    }

    pub(crate) fn contains(self, other: Self) -> bool {
        self.start <= other.start && other.end <= self.end
    }

    pub(super) fn overlaps(self, other: Self) -> bool {
        self.start < other.end && other.start < self.end
    }

    /// Returns the parts of this range outside all `excluded` ranges.
    pub(super) fn excluding(self, excluded: &[Self]) -> Vec<Self> {
        let mut subtracted: Vec<_> = excluded
            .iter()
            .filter_map(|range| {
                let start = self.start.max(range.start);
                let end = self.end.min(range.end);
                (start < end).then_some(Self { start, end })
            })
            .collect();
        subtracted.sort_unstable_by_key(|range| range.start());

        let mut result = Vec::new();
        let mut cursor = self.start;
        for range in subtracted {
            if cursor < range.start {
                result.push(Self {
                    start: cursor,
                    end: range.start,
                });
            }
            cursor = cursor.max(range.end);
        }
        if cursor < self.end {
            result.push(Self {
                start: cursor,
                end: self.end,
            });
        }
        result
    }

    pub(super) fn adjacent(self, other: Self) -> bool {
        self.end == other.start || other.end == self.start
    }

    pub(super) fn join(self, other: Self) -> Self {
        let start = if self.start < other.start {
            self.start
        } else {
            other.start
        };
        let end = if self.end > other.end {
            self.end
        } else {
            other.end
        };
        Self { start, end }
    }

    /// Returns the non-empty subrange beginning `offset` bytes from `start`.
    ///
    /// Returns [`Error::InvalidArgs`] when the requested bytes extend beyond
    /// this range or `len` is zero, and [`Error::Overflow`] when either address
    /// calculation cannot be represented.
    pub(super) fn subrange(self, offset: usize, len: usize) -> Result<Self> {
        if len == 0 {
            return Err(Error::InvalidArgs);
        }
        let start = self.start.checked_add(offset).ok_or(Error::Overflow)?;
        let end = start.checked_add(len).ok_or(Error::Overflow)?;
        if end > self.end {
            return Err(Error::InvalidArgs);
        }
        Ok(Self { start, end })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_reject_empty_and_wrapping_inputs() {
        assert_eq!(
            PhysAddrRange::new(PhysAddr::new(1), PhysAddr::new(1)),
            Err(Error::InvalidArgs)
        );
        assert_eq!(
            PhysAddrRange::new(PhysAddr::new(2), PhysAddr::new(1)),
            Err(Error::InvalidArgs)
        );
        assert_eq!(
            PhysAddrRange::from_start_len(PhysAddr::new(usize::MAX), 2),
            Err(Error::Overflow)
        );
    }

    #[test]
    fn subranges_reject_empty_overflowing_and_out_of_bounds_requests() {
        let range = PhysAddrRange::from_start_len(PhysAddr::new(0x1000), 0x100).unwrap();
        assert_eq!(range.subrange(0, 0), Err(Error::InvalidArgs));
        assert_eq!(range.subrange(usize::MAX, 1), Err(Error::Overflow));
        assert_eq!(range.subrange(0x80, 0x81), Err(Error::InvalidArgs));
    }
}
