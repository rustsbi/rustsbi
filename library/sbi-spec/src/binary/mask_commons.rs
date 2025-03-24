//! Common SBI mask operations and structures.

/// Check if the implementation can contains the provided `bit`.
#[inline]
pub(crate) const fn valid_bit(base: usize, bit: usize) -> bool {
    if bit < base {
        // invalid index, under minimum range.
        false
    } else if (bit - base) >= usize::BITS as usize {
        // invalid index, over max range.
        false
    } else {
        true
    }
}

/// Check if the implementation contains the provided `bit`.
///
/// ## Parameters
///
/// - `mask`: bitmask defining the range of bits.
/// - `base`: the starting bit index. (default: `0`)
/// - `ignore`: if `base` is equal to this value, ignore the `mask` parameter, and consider all `bit`s set.
/// - `bit`: the bit index to check for membership in the `mask`.
#[inline]
pub(crate) const fn has_bit(mask: usize, base: usize, ignore: usize, bit: usize) -> bool {
    if base == ignore {
        // ignore the `mask`, consider all `bit`s as set.
        true
    } else if !valid_bit(base, bit) {
        false
    } else {
        // index is in range, check if it is set in the mask.
        mask & (1 << (bit - base)) != 0
    }
}

/// Error of mask modification.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum MaskError {
    /// This mask has been ignored.
    Ignored,
    /// Request bit is invalid.
    InvalidBit,
}
