//! Machine interrupt-enable bits (mie)

riscv::read_write_csr! {
    /// Machine interrupt-enable bits.
    Mie: 0x304,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}

impl Mie {
    #[inline]
    pub const fn bit(self, n: usize) -> bool {
        ((self.bits >> n) & 1) != 0
    }

    /// Supervisor software interrupt enable (bit 1).
    #[inline]
    pub const fn ssip(self) -> bool {
        self.bit(1)
    }

    /// Supervisor timer interrupt enable (bit 5).
    #[inline]
    pub const fn stip(self) -> bool {
        self.bit(5)
    }

    /// Supervisor external interrupt enable (bit 9).
    #[inline]
    pub const fn seip(self) -> bool {
        self.bit(9)
    }

    /// Machine software interrupt enable (bit 3).
    #[inline]
    pub const fn msip(self) -> bool {
        self.bit(3)
    }

    /// Machine timer interrupt enable (bit 7).
    #[inline]
    pub const fn mtip(self) -> bool {
        self.bit(7)
    }

    /// Machine external interrupt enable (bit 11).
    #[inline]
    pub const fn meip(self) -> bool {
        self.bit(11)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::register::mip::Mip;

    #[test]
    fn mie_mip_delegate_bits() {
        // ensure mie/mip still behave as expected alongside delegation
        let bits: usize = (1usize << 3) | (1usize << 7) | (1usize << 11);
        let en = Mie::from_bits(bits);
        let pd = Mip::from_bits(bits);
        assert!(en.msip());
        assert!(en.mtip());
        assert!(en.meip());
        assert!(pd.msip());
        assert!(pd.mtip());
        assert!(pd.meip());
    }

    #[test]
    fn mie_parsing_bits() {
        // set msip (bit 3), mtip (bit 7), meip (bit 11)
        let bits: usize = (1usize << 3) | (1usize << 7) | (1usize << 11);
        let reg = Mie::from_bits(bits);
        assert!(reg.msip());
        assert!(reg.mtip());
        assert!(reg.meip());
        // other bits like ssip should be false
        assert!(!reg.ssip());
    }
}
