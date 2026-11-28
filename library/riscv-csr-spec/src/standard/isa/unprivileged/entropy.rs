//! Scalar cryptographic entropy source.

csr_value! {
    /// Cryptographic scalar entropy word returned by `csrrw rd, seed, x0` (`0x015`).
    pub struct SeedBits(u32);
}

/// Operational polling status of the `Zkr` entropy source (`seed.OPST`, bits `31:30`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SeedStatus {
    /// Startup self-test or a nonfatal self-test alarm (`0b00`).
    Bist,
    /// Insufficient entropy accumulated yet (`0b01`).
    Wait,
    /// A 16-bit entropy-source sample is available in `seed[15:0]` (`0b10`).
    Es16,
    /// Unrecoverable self-test error (`0b11`).
    Dead,
}

impl SeedBits {
    /// Decodes the 2-bit `OPST` status (`seed[31:30]`).
    #[inline(always)]
    pub const fn status(self) -> SeedStatus {
        match (self.0 >> 30) & 0b11 {
            0b00 => SeedStatus::Bist,
            0b01 => SeedStatus::Wait,
            0b10 => SeedStatus::Es16,
            _ => SeedStatus::Dead,
        }
    }

    /// Extracts the 16-bit entropy payload (`seed[15:0]`, valid only when `status() == SeedStatus::Es16`).
    #[inline(always)]
    pub const fn entropy(self) -> u16 {
        (self.0 & 0xffff) as u16
    }
}

define_csr! {
    /// Scalar cryptographic entropy source (`seed`, `Zkr`).
    ///
    /// Zkr requires a CSR instruction that writes. The written value is ignored.
    /// `claim()` uses `csrrw rd, seed, x0` to poll and consumes the sample
    /// when the returned status is [`SeedStatus::Es16`]. A read-only CSR access traps.
    ///
    /// # Access requirements
    ///
    /// Hardware polling requires Zkr. S/HS access requires `mseccfg.SSEED`,
    /// and nonvirtual U access requires `mseccfg.USEED`. Guest accesses trap.
    ///
    /// Reference: <https://docs.riscv.org/reference/isa/v20260120/unpriv/scalar-crypto.html#crypto_scalar_es_access>.
    /// [Privilege checks](https://docs.riscv.org/reference/isa/v20260120/priv/machine.html#sec:mseccfg).
    Seed { 0x015, claim: SeedBits };
}
