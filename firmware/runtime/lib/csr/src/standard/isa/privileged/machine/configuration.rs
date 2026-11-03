//! Machine configuration.
use crate::macros::csr;
use crate::value::{Value, WriteField};
use crate::{Read, Write};

/// A legal RV64 supervisor pointer-masking mode.
///
/// See <https://docs.riscv.org/reference/isa/priv/zpm.html>.
#[repr(usize)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PointerMasking {
    Disabled = 0,
    Bits7 = 2,
    Bits16 = 3,
}

/// A pointer-masking read encoding, including reserved values.
///
/// See <https://docs.riscv.org/reference/isa/priv/zpm.html>.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PointerMaskingRead {
    Disabled,
    Bits7,
    Bits16,
    Unknown(usize),
}

impl Value for PointerMasking {
    const BITS: u32 = 2;
    #[inline]
    fn from_bits(bits: usize) -> Self {
        match bits {
            0 => Self::Disabled,
            2 => Self::Bits7,
            3 => Self::Bits16,
            _ => unreachable!("reserved pointer-masking write encoding"),
        }
    }
    #[inline]
    fn bits(self) -> usize {
        self as usize
    }
}

impl Value for PointerMaskingRead {
    const BITS: u32 = 2;
    #[inline]
    fn from_bits(bits: usize) -> Self {
        match bits {
            0 => Self::Disabled,
            2 => Self::Bits7,
            3 => Self::Bits16,
            raw => Self::Unknown(raw),
        }
    }
    #[inline]
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
        /// Writes the upper half of the environment configuration.
        ///
        /// # Safety
        ///
        /// Access only on RV32. Preserve the lower-mode execution and translation
        /// context, including active `MPRV` accesses, and coordinate conflicting
        /// configuration changes.
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
    /// Sets a security field.
    ///
    /// This uses `CSRRS`, which reads and writes the register. Bits that read
    /// differently from their underlying values can be changed by the update.
    /// See <https://docs.riscv.org/reference/isa/unpriv/zicsr.html>.
    ///
    /// # Safety
    ///
    /// Execute in M-mode on a hart with `mseccfg`. The caller must satisfy the
    /// selected field's security-policy requirements and coordinate other
    /// firmware users of this register.
    #[inline]
    pub unsafe fn set_bit<F: WriteField<SecurityConfig<Write>, Value = FeatureState>>() {
        let mut mask = SecurityConfig::<Write>::default();
        mask.set::<F>(FeatureState::Enabled);
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        // SAFETY: The caller coordinates this update and satisfies the
        // selected field's security-policy requirements.
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

impl From<EnvironmentConfig<Read>> for EnvironmentConfig<Write> {
    #[inline]
    fn from(value: EnvironmentConfig<Read>) -> Self {
        Self::from_raw(value.0)
    }
}

impl From<EnvironmentConfigHigh<Read>> for EnvironmentConfigHigh<Write> {
    #[inline]
    fn from(value: EnvironmentConfigHigh<Read>) -> Self {
        Self::from_raw(value.0)
    }
}

impl From<bool> for FeatureState {
    #[inline]
    fn from(enabled: bool) -> Self {
        if enabled {
            Self::Enabled
        } else {
            Self::Disabled
        }
    }
}

impl PointerMasking {
    /// Returns the mode for a supported masked pointer length, if any.
    #[inline]
    pub const fn from_pmlen(length: usize) -> Option<Self> {
        match length {
            0 => Some(Self::Disabled),
            7 => Some(Self::Bits7),
            16 => Some(Self::Bits16),
            _ => None,
        }
    }

    /// Returns the masked pointer length, independently of its CSR encoding.
    #[inline]
    pub const fn pmlen(self) -> usize {
        match self {
            Self::Disabled => 0,
            Self::Bits7 => 7,
            Self::Bits16 => 16,
        }
    }
}
