//! Supervisor interrupt-pending high-half (siph) (RV32 only)

riscv::read_write_csr! {
    /// Upper 32 bits of sip (RV32 only).
    Siph: 0x154,
    mask: 0xFFFF_FFFF,
}

impl Siph {
    #[inline]
    pub const fn raw(self) -> usize {
        self.bits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn siph_raw_roundtrip() {
        let bits: usize = 0x55AA55AAusize & 0xFFFF_FFFF;
        let reg = Siph::from_bits(bits);
        assert_eq!(reg.raw(), bits);
    }
}
