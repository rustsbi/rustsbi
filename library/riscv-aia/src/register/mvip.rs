//! Machine virtual interrupt pending bits (mvip)

riscv::read_write_csr! {
    /// Machine virtual interrupt pending bits.
    Mvip: 0x309,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}

impl Mvip {
    /// Supervisor software interrupt pending in mvip (bit 1).
    #[inline]
    pub const fn ssip(self) -> bool {
        ((self.bits >> 1) & 1) != 0
    }

    /// Supervisor timer interrupt pending in mvip (bit 5).
    #[inline]
    pub const fn stip(self) -> bool {
        ((self.bits >> 5) & 1) != 0
    }

    /// Supervisor external interrupt pending in mvip (bit 9).
    #[inline]
    pub const fn seip(self) -> bool {
        ((self.bits >> 9) & 1) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mvip_bits() {
        let bits: usize = (1usize << 1) | (1usize << 5) | (1usize << 9);
        let reg = Mvip::from_bits(bits);
        assert!(reg.ssip());
        assert!(reg.stip());
        assert!(reg.seip());
    }

    #[test]
    fn mvip_zero() {
        let reg = Mvip::from_bits(0);
        assert!(!reg.ssip());
        assert!(!reg.stip());
        assert!(!reg.seip());
    }
}
