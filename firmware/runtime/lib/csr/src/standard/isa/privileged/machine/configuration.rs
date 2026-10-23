//! Machine configuration.
use crate::macros::csr;

/// A legal RV64 supervisor pointer-masking mode.
#[repr(usize)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PointerMasking {
    Disabled = 0,
    Bits7 = 2,
    Bits16 = 3,
}

/// A pointer-masking read encoding, including reserved values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PointerMaskingRead {
    Disabled,
    Bits7,
    Bits16,
    Unknown(usize),
}

impl crate::value::Value for PointerMasking {
    const BITS: u32 = 2;
    fn from_bits(bits: usize) -> Self {
        match bits {
            0 => Self::Disabled,
            2 => Self::Bits7,
            3 => Self::Bits16,
            _ => unreachable!("reserved pointer-masking write encoding"),
        }
    }
    fn bits(self) -> usize {
        self as usize
    }
}

impl crate::value::Value for PointerMaskingRead {
    const BITS: u32 = 2;
    fn from_bits(bits: usize) -> Self {
        match bits {
            0 => Self::Disabled,
            2 => Self::Bits7,
            3 => Self::Bits16,
            raw => Self::Unknown(raw),
        }
    }
    fn bits(self) -> usize {
        match self {
            Self::Disabled => 0,
            Self::Bits7 => 2,
            Self::Bits16 => 3,
            Self::Unknown(raw) => raw,
        }
    }
}

csr! {
    /// Machine environment configuration register (`menvcfg`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/machine.html#menvcfg>.
    EnvironmentConfig { 0x30a, r,
        /// See
        /// <https://docs.riscv.org/reference/isa/priv/machine.html#menvcfg>.
        ///
        /// # Safety
        ///
        /// Preserve the lower-mode execution and translation context, including
        /// the state used by `MPRV` accesses and the next privilege transition.
        unsafe w
    }
        LPE[2] { rw: FeatureState { Disabled = 0, Enabled = 1 } }
        SSE[3] { rw: FeatureState }
        CBIE[4..=5] {
            r: CacheBlockInvalidate { Disabled = 0, Flush = 1, Invalidate = 3 },
            w: CacheBlockInvalidateMode { Disabled = 0, Flush = 1, Invalidate = 3 }
        }
        CBCFE[6] { rw: FeatureState }
        CBZE[7] { rw: FeatureState }
        #[cfg(target_pointer_width = "64")]
        PMM[32..=33] { r: PointerMaskingRead, w: PointerMasking }
        #[cfg(target_pointer_width = "64")]
        DTE[59] { rw: FeatureState }
        #[cfg(target_pointer_width = "64")]
        ADUE[61] { rw: FeatureState }
        #[cfg(target_pointer_width = "64")]
        PBMTE[62] { rw: FeatureState }
        #[cfg(target_pointer_width = "64")]
        STCE[63] { rw: FeatureState }
    /// Upper half of `menvcfg` on RV32.
    EnvironmentConfigHigh { 0x31a, r,
        /// Preserves execution and translation invariants as for `menvcfg`.
        ///
        /// # Safety
        ///
        /// Access only on RV32. Preserve the lower-mode execution and translation
        /// context, including active `MPRV` accesses.
        /// Preserve Runtime's firmware-feature locks.
        unsafe w
    }
        DTEH[27] { rw: FeatureState }
        ADUEH[29] { rw: FeatureState }
        PBMTEH[30] { rw: FeatureState }
        STCEH[31] { rw: FeatureState }
    /// Machine security configuration register (`mseccfg`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/machine.html#mseccfg>.
    SecurityConfig { 0x747, r }
        SSEED[9] { rw: FeatureState }
}

impl SecurityConfig {
    /// Grants S-mode access to `seed` with one CSRRS instruction.
    /// Other security and PMP lock fields retain their values.
    ///
    /// # Safety
    /// Run on a hart with mseccfg during initial security-policy setup.
    pub unsafe fn enable_supervisor_seed() {
        let mut mask = SecurityConfigWriteValue(0);
        mask.write::<SSEED>(FeatureState::Enabled);
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        // SAFETY: the caller owns this grant; CSRRS preserves unrelated locks.
        unsafe {
            core::arch::asm!("csrs {csr}, {mask}", csr = const Self::NUMBER,
            mask = in(reg) usize::from(mask), options(nostack));
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        {
            let _ = mask;
            panic!("CSR access requires RISC-V");
        }
    }
}

impl EnvironmentConfigReadValue {
    /// Returns whether Sstc controls supervisor timer pending state on RV64.
    #[cfg(target_pointer_width = "64")]
    pub fn supervisor_timer_enabled(self) -> bool {
        self.read::<STCE>() == FeatureState::Enabled
    }
}

impl EnvironmentConfigHighReadValue {
    /// Returns whether Sstc controls supervisor timer pending state on RV32.
    pub fn supervisor_timer_enabled(self) -> bool {
        self.read::<STCEH>() == FeatureState::Enabled
    }
}

impl From<EnvironmentConfigReadValue> for EnvironmentConfigWriteValue {
    fn from(value: EnvironmentConfigReadValue) -> Self {
        Self(value.0)
    }
}

impl From<EnvironmentConfigHighReadValue> for EnvironmentConfigHighWriteValue {
    fn from(value: EnvironmentConfigHighReadValue) -> Self {
        Self(value.0)
    }
}

impl From<bool> for FeatureState {
    fn from(enabled: bool) -> Self {
        if enabled {
            Self::Enabled
        } else {
            Self::Disabled
        }
    }
}

impl PointerMasking {
    /// Converts a requested masked pointer length to its legal CSR encoding.
    pub const fn from_pmlen(length: usize) -> Option<Self> {
        match length {
            0 => Some(Self::Disabled),
            7 => Some(Self::Bits7),
            16 => Some(Self::Bits16),
            _ => None,
        }
    }

    /// Returns the masked pointer length, independently of its CSR encoding.
    pub const fn pmlen(self) -> usize {
        match self {
            Self::Disabled => 0,
            Self::Bits7 => 7,
            Self::Bits16 => 16,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Value;

    #[test]
    fn low_fields_preserve_unrelated_bits_and_reserved_encodings() {
        let original = EnvironmentConfigReadValue::from_bits((1 << 0) | (2 << 4));
        let mut value = EnvironmentConfigWriteValue::from(original);
        value.write::<LPE>(FeatureState::Enabled);
        value.write::<SSE>(FeatureState::Enabled);
        assert_eq!(
            usize::from(value),
            (1 << 0) | (2 << 4) | (1 << 2) | (1 << 3)
        );
        assert_eq!(value.read::<CBIE>(), CacheBlockInvalidate::Unknown(2));
    }

    #[test]
    fn rv32_high_fields_match_the_architectural_positions() {
        let mut value = EnvironmentConfigHighWriteValue::from_bits(0);
        value.write::<DTEH>(FeatureState::Enabled);
        value.write::<ADUEH>(FeatureState::Enabled);
        value.write::<STCEH>(FeatureState::Enabled);
        assert_eq!(usize::from(value), (1 << 27) | (1 << 29) | (1 << 31));
    }

    #[test]
    fn pointer_lengths_are_not_raw_pmm_encodings() {
        for (length, bits) in [(0, 0), (7, 2), (16, 3)] {
            let mode = PointerMasking::from_pmlen(length).unwrap();
            assert_eq!(mode.pmlen(), length);
            assert_eq!(mode.bits(), bits);
        }
        assert_eq!(PointerMasking::from_pmlen(1), None);
        assert_eq!(PointerMasking::from_pmlen(2), None);
        assert_eq!(PointerMasking::from_pmlen(3), None);
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn rv64_high_fields_and_pointer_masking_use_the_upper_word() {
        let mut value = EnvironmentConfigWriteValue::from_bits(0);
        value.write::<PMM>(PointerMasking::Bits7);
        value.write::<DTE>(FeatureState::Enabled);
        value.write::<ADUE>(FeatureState::Enabled);
        value.write::<STCE>(FeatureState::Enabled);
        assert_eq!(
            usize::from(value),
            (2 << 32) | (1 << 59) | (1 << 61) | (1 << 63)
        );
    }
}
