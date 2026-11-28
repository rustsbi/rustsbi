//! Common SBI mask operations and structures.

/// Returns whether `bit` fits in a mask starting at `base`.
#[inline]
pub(crate) const fn valid_bit(base: usize, bit: usize) -> bool {
    bit >= base && bit - base < usize::BITS as usize
}

/// Tests the bit at offset `bit - base` in a single register.
#[inline]
pub(crate) const fn has_bit(mask: usize, base: usize, bit: usize) -> bool {
    valid_bit(base, bit) && mask & (1 << (bit - base)) != 0
}

/// Error of mask modification.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum MaskError {
    /// This mask has been ignored.
    Ignored,
    /// Request bit is invalid.
    InvalidBit,
}
