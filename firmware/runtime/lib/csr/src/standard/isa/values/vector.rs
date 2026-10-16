//! Trap vector modes and aligned entry addresses.
use super::Address;
use crate::value::Value;

/// Trap dispatch mode read from hardware, including reserved encodings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VectorMode {
    /// All traps enter at the base address.
    Direct,
    /// Interrupts enter at the base plus four times the cause code.
    Vectored,
    /// An encoding not defined by the base privileged specification.
    Unknown(usize),
}

/// Standard trap dispatch modes accepted for writes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TargetVectorMode {
    /// All traps enter at the base address.
    Direct,
    /// Interrupts enter at the base plus four times the cause code.
    Vectored,
}

impl Value for VectorMode {
    const BITS: u32 = 2;
    fn from_bits(bits: usize) -> Self {
        match bits {
            0 => Self::Direct,
            1 => Self::Vectored,
            raw => Self::Unknown(raw),
        }
    }
    fn bits(self) -> usize {
        match self {
            Self::Direct => 0,
            Self::Vectored => 1,
            Self::Unknown(raw) => raw,
        }
    }
}

impl Value for TargetVectorMode {
    const BITS: u32 = 2;
    fn from_bits(bits: usize) -> Self {
        match bits {
            0 => Self::Direct,
            1 => Self::Vectored,
            _ => unreachable!(),
        }
    }
    fn bits(self) -> usize {
        match self {
            Self::Direct => 0,
            Self::Vectored => 1,
        }
    }
}

/// A four-byte-aligned trap entry address, encoded in a vector's BASE field.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VectorBase(usize);

impl VectorBase {
    /// Checks four-byte alignment; vectored mode may require stricter hardware alignment.
    pub const fn new(address: Address) -> Option<Self> {
        if address.0 & 3 == 0 {
            Some(Self(address.0))
        } else {
            None
        }
    }
    /// Returns the byte address before BASE-field encoding.
    pub const fn address(self) -> Address {
        Address(self.0)
    }
}

impl Value for VectorBase {
    const BITS: u32 = usize::BITS - 2;
    fn from_bits(bits: usize) -> Self {
        Self(bits << 2)
    }
    fn bits(self) -> usize {
        self.0 >> 2
    }
}
