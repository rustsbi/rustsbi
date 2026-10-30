//! Architectural register widths.
//!
//! The `misa.MXL` encoding is also used by `mstatus.SXL`, `mstatus.UXL`,
//! and `hstatus.VSXL`. See:
//! <https://docs.riscv.org/reference/isa/priv/machine.html#misa> and
//! <https://docs.riscv.org/reference/isa/priv/hypervisor.html#hstatus>.
use crate::value::Value;

/// Nonzero integer register width encodings read from hardware.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Xlen {
    X32,
    X64,
    X128,
}

/// Integer register widths supported for writes by this RV32/RV64 crate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TargetXlen {
    X32,
    X64,
}

// SXL/UXL read as zero when the corresponding privilege mode is absent.
// Preserve zero as None when decoding other fields that use this encoding.
impl Value for Option<Xlen> {
    const BITS: u32 = 2;
    #[inline]
    fn from_bits(bits: usize) -> Self {
        match bits {
            0 => None,
            1 => Some(Xlen::X32),
            2 => Some(Xlen::X64),
            3 => Some(Xlen::X128),
            _ => unreachable!("XLEN field exceeds two bits"),
        }
    }
    #[inline]
    fn bits(self) -> usize {
        match self {
            None => 0,
            Some(Xlen::X32) => 1,
            Some(Xlen::X64) => 2,
            Some(Xlen::X128) => 3,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_encodings_distinguish_absent_and_supported_widths() {
        for (bits, width) in [
            (0, None),
            (1, Some(Xlen::X32)),
            (2, Some(Xlen::X64)),
            (3, Some(Xlen::X128)),
        ] {
            assert_eq!(Option::<Xlen>::from_bits(bits), width);
            assert_eq!(width.bits(), bits);
        }
    }
}

impl Value for TargetXlen {
    const BITS: u32 = 2;
    #[inline]
    fn from_bits(bits: usize) -> Self {
        match bits {
            1 => Self::X32,
            2 => Self::X64,
            _ => unreachable!(),
        }
    }
    #[inline]
    fn bits(self) -> usize {
        match self {
            Self::X32 => 1,
            Self::X64 => 2,
        }
    }
}
