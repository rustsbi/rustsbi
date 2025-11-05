//! Supervisor interrupt-enable bits (sie)

riscv::read_write_csr! {
    /// Supervisor interrupt-enable bits.
    Sie: 0x104,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}

impl Sie {
    /// Test bit `n` of `sie`.
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sie_bits() {
        let bits: usize = (1usize << 1) | (1usize << 5) | (1usize << 9);
        let reg = Sie::from_bits(bits);
        assert!(reg.ssip());
        assert!(reg.stip());
        assert!(reg.seip());
    }

    #[test]
    fn sie_zero() {
        let reg = Sie::from_bits(0);
        assert!(!reg.ssip());
        assert!(!reg.stip());
        assert!(!reg.seip());
    }
}
