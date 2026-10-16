//! Architectural register widths.
use crate::value::Value;

/// Nonzero integer register width encodings read from hardware.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Xlen {
    X32,
    X64,
    /// Encoding 3, reserved by the privileged specification.
    Reserved3,
}

/// Standard integer register widths accepted for writes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TargetXlen {
    X32,
    X64,
}

// Zero reports an unimplemented width field rather than a register width.
impl Value for Option<Xlen> {
    const BITS: u32 = 2;
    fn from_bits(bits: usize) -> Self {
        match bits {
            0 => None,
            1 => Some(Xlen::X32),
            2 => Some(Xlen::X64),
            3 => Some(Xlen::Reserved3),
            _ => unreachable!("XLEN field exceeds two bits"),
        }
    }
    fn bits(self) -> usize {
        match self {
            None => 0,
            Some(Xlen::X32) => 1,
            Some(Xlen::X64) => 2,
            Some(Xlen::Reserved3) => 3,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_encodings_distinguish_absent_and_reserved_fields() {
        for (bits, width) in [
            (0, None),
            (1, Some(Xlen::X32)),
            (2, Some(Xlen::X64)),
            (3, Some(Xlen::Reserved3)),
        ] {
            assert_eq!(Option::<Xlen>::from_bits(bits), width);
            assert_eq!(width.bits(), bits);
        }
    }
}

impl Value for TargetXlen {
    const BITS: u32 = 2;
    fn from_bits(bits: usize) -> Self {
        match bits {
            1 => Self::X32,
            2 => Self::X64,
            _ => unreachable!(),
        }
    }
    fn bits(self) -> usize {
        match self {
            Self::X32 => 1,
            Self::X64 => 2,
        }
    }
}
