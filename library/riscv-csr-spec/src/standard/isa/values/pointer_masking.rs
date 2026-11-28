//! RV64 pointer-masking controls for `menvcfg`, `senvcfg`, and `henvcfg`.
//!
//! Reference: <https://docs.riscv.org/reference/isa/v20260120/priv/zpm.html#pointer-mask-extensions>.

field_enum! {
    /// Pointer-masking mode encoded in `PMM`.
    ///
    /// Models `Smnpm` (`menvcfg`) and `Ssnpm` (`senvcfg` and `henvcfg`).
    pub enum PointerMasking {
        /// Leaves pointers unchanged by pointer masking.
        Disabled = 0,
        /// Masks the top seven pointer bits for covered accesses.
        Bits7 = 2,
        /// Masks the top sixteen pointer bits for covered accesses.
        Bits16 = 3,
    }
}

impl PointerMasking {
    /// Converts a pointer mask length (`0`, `7`, or `16`) to its CSR encoding.
    #[inline(always)]
    pub const fn from_pmlen(pmlen: usize) -> Option<Self> {
        match pmlen {
            0 => Some(Self::Disabled),
            7 => Some(Self::Bits7),
            16 => Some(Self::Bits16),
            _ => None,
        }
    }

    /// Returns the masked pointer tag width in bits (`0`, `7`, or `16`).
    #[inline(always)]
    pub const fn pmlen(self) -> usize {
        match self {
            Self::Disabled => 0,
            Self::Bits7 => 7,
            Self::Bits16 => 16,
        }
    }
}
