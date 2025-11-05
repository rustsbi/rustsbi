//! Hypervisor virtual interrupt enables high-half (hvienh) (RV32 only)

riscv::read_write_csr! {
    /// Upper 32 bits of hvien.
    Hvienh: 0x618,
    mask: 0xFFFF_FFFF,
}

impl Hvienh {
    #[inline]
    pub const fn raw(self) -> usize {
        self.bits
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::register::hviph::Hviph;

    #[test]
    fn hvienh_hviph_raw_roundtrip() {
        let bits: usize = 0xDEAD_BEEFusize & 0xFFFF_FFFF;
        let en = Hvienh::from_bits(bits);
        let p = Hviph::from_bits(bits);
        assert_eq!(en.raw(), bits);
        assert_eq!(p.raw(), bits);
    }
}
