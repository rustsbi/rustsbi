//! Conversion between a declared CSR value and its instruction operand.

pub(crate) trait Value: Copy {
    const BITS: u32;
    fn from_bits(bits: usize) -> Self;
    fn bits(self) -> usize;
}

impl Value for usize {
    const BITS: u32 = usize::BITS;

    #[inline]
    fn from_bits(bits: usize) -> Self {
        bits
    }

    #[inline]
    fn bits(self) -> usize {
        self
    }
}

impl Value for u32 {
    const BITS: u32 = u32::BITS;

    #[inline]
    fn from_bits(bits: usize) -> Self {
        bits as Self
    }

    #[inline]
    fn bits(self) -> usize {
        self as usize
    }
}
