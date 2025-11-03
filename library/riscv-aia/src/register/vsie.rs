//! Virtual supervisor interrupt-enable bits (vsie)

riscv::read_write_csr! {
    /// Virtual supervisor interrupt-enable bits.
    Vsie: 0x204,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}

impl Vsie {
    /// Test bit `n` of `vsie`.
    #[inline]
    pub const fn bit(self, n: usize) -> bool {
        ((self.bits >> n) & 1) != 0
    }

    /// VS-level software interrupt enable (bit 2).
    #[inline]
    pub const fn vssip(self) -> bool {
        self.bit(2)
    }

    /// VS-level timer interrupt enable (bit 6).
    #[inline]
    pub const fn vstip(self) -> bool {
        self.bit(6)
    }

    /// VS-level external interrupt enable (bit 10).
    #[inline]
    pub const fn vseip(self) -> bool {
        self.bit(10)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::register::vsip::Vsip;

    #[test]
    fn vsie_vsip_parsing_bits() {
        // set vssip (bit 2), vstip (bit 6), vseip (bit 10)
        let bits: usize = (1usize << 2) | (1usize << 6) | (1usize << 10);
        let en = Vsie::from_bits(bits);
        let pend = Vsip::from_bits(bits);
        assert!(en.vssip());
        assert!(en.vstip());
        assert!(en.vseip());
        assert!(pend.vssip());
        assert!(pend.vstip());
        assert!(pend.vseip());
    }
}
