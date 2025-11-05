//! Supervisor interrupt-pending bits (sip)

riscv::read_write_csr! {
    /// Supervisor interrupt-pending bits.
    Sip: 0x144,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}

impl Sip {
    /// Test bit `n` of `sip`.
    #[inline]
    pub const fn bit(self, n: usize) -> bool {
        ((self.bits >> n) & 1) != 0
    }

    /// Supervisor software interrupt pending (bit 1).
    #[inline]
    pub const fn ssip(self) -> bool {
        self.bit(1)
    }

    /// Supervisor timer interrupt pending (bit 5).
    #[inline]
    pub const fn stip(self) -> bool {
        self.bit(5)
    }

    /// Supervisor external interrupt pending (bit 9).
    #[inline]
    pub const fn seip(self) -> bool {
        self.bit(9)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sip_bits() {
        let bits: usize = (1usize << 1) | (1usize << 5) | (1usize << 9);
        let reg = Sip::from_bits(bits);
        assert!(reg.ssip());
        assert!(reg.stip());
        assert!(reg.seip());
    }

    #[test]
    fn sip_zero() {
        let reg = Sip::from_bits(0);
        assert!(!reg.ssip());
        assert!(!reg.stip());
        assert!(!reg.seip());
    }
}
