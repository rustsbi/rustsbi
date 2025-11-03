//! Virtual supervisor indirect register alias (vsireg)

riscv::read_write_csr! {
    /// Virtual supervisor indirect register alias.
    Vsireg: 0x251,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}

impl Vsireg {
    /// Raw bits read from `vsireg` (width depends on XLEN).
    #[inline]
    pub const fn raw(self) -> usize {
        self.bits
    }

    /// Convenience accessor returning bits as usize.
    #[inline]
    pub const fn as_usize(self) -> usize {
        self.bits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vsireg_raw_roundtrip() {
        let bits: usize = 0xABCD_EF01usize & 0xFFFF_FFFF_FFFF_FFFF;
        let reg = Vsireg::from_bits(bits);
        assert_eq!(reg.raw(), bits);
        assert_eq!(reg.as_usize(), bits);
    }
}
