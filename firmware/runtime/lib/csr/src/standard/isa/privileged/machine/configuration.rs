//! Machine configuration.
use crate::macros::csr;

csr! {
    pub struct EnvironmentConfig(usize);
    pub struct EnvironmentConfigHigh(usize);
    pub struct SecurityConfig(usize);

    /// Machine environment configuration register (`menvcfg`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/machine.html#menvcfg>.
    EnvironmentConfiguration { 0x30a, r: EnvironmentConfig,
        /// See
        /// <https://docs.riscv.org/reference/isa/priv/machine.html#menvcfg>.
        ///
        /// # Safety
        ///
        /// Preserve the lower-mode execution and translation context, including
        /// the state used by `MPRV` accesses and the next privilege transition.
        unsafe w: EnvironmentConfig
    }
    /// Upper half of `menvcfg` on RV32.
    EnvironmentConfigurationHigh { 0x31a, r: EnvironmentConfigHigh }
    /// Machine security configuration register (`mseccfg`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/machine.html#mseccfg>.
    SecurityConfiguration { 0x747, r: SecurityConfig }
}

impl EnvironmentConfig {
    /// Returns whether Sstc controls supervisor timer pending state on RV64.
    #[cfg(target_pointer_width = "64")]
    pub const fn supervisor_timer_enabled(self) -> bool {
        self.0 & (1usize << 63) != 0
    }
}

impl EnvironmentConfigHigh {
    /// Returns whether Sstc controls supervisor timer pending state on RV32.
    pub const fn supervisor_timer_enabled(self) -> bool {
        self.0 & (1usize << 31) != 0
    }
}

impl From<usize> for EnvironmentConfig {
    fn from(bits: usize) -> Self {
        Self(bits)
    }
}

impl From<usize> for EnvironmentConfigHigh {
    fn from(bits: usize) -> Self {
        Self(bits)
    }
}
