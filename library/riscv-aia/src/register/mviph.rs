//! Machine virtual interrupt pending high-half (mviph) (RV32 only)

riscv::read_write_csr! {
    /// Upper 32 bits of mvip (RV32 only).
    Mviph: 0x319,
    mask: 0xFFFF_FFFF,
}

impl Mviph {
    #[inline]
    pub const fn raw(self) -> usize {
        self.bits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mviph_raw_roundtrip() {
        let bits: usize = 0x0F0F0F0Fusize & 0xFFFF_FFFF;
        let reg = Mviph::from_bits(bits);
        assert_eq!(reg.raw(), bits);
    }
}
