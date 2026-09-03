//! Non-empty physical-address ranges.

use super::PhysAddr;
use crate::{Error, Result};

/// A non-empty half-open range of physical addresses.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhysAddrRange {
    start: PhysAddr,
    end: PhysAddr,
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

    pub(crate) fn contains(self, other: Self) -> bool {
        self.start <= other.start && other.end <= self.end
    }

    pub(crate) fn overlaps(self, other: Self) -> bool {
        self.start < other.end && other.start < self.end
    }

    pub(crate) fn adjacent(self, other: Self) -> bool {
        self.end == other.start || other.end == self.start
    }

    pub(crate) fn join(self, other: Self) -> Self {
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

    pub(crate) fn subrange(self, offset: usize, len: usize) -> Result<Self> {
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
