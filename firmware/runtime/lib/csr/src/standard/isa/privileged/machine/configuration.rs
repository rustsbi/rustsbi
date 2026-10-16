//! Machine configuration.
use crate::macros::csr;

csr! {
    pub struct EnvironmentConfig(usize);
    pub struct SecurityConfig(usize);

    /// Environment controls for lower privilege modes (`menvcfg`).
    ///
    /// <https://docs.riscv.org/reference/isa/priv/machine.html#menvcfg>
    EnvironmentConfiguration { 0x30a, r: EnvironmentConfig,
        /// <https://docs.riscv.org/reference/isa/priv/machine.html#menvcfg>
        ///
        /// # Safety
        ///
        /// Preserve the lower-mode execution and translation context, including
        /// the state used by `MPRV` accesses and the next privilege transition.
        unsafe w: EnvironmentConfig
    }
    /// Machine security controls (`mseccfg`).
    ///
    /// <https://docs.riscv.org/reference/isa/priv/machine.html#mseccfg>
    SecurityConfiguration { 0x747, r: SecurityConfig }
}

impl From<usize> for EnvironmentConfig {
    fn from(bits: usize) -> Self {
        Self(bits)
    }
}
