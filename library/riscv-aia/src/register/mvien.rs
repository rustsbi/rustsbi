//! Machine virtual interrupt enables (mvien)

riscv::read_write_csr! {
    /// Machine virtual interrupt enables.
    Mvien: 0x308,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}

impl Mvien {
    /// Supervisor software interrupt enable in mvien (bit 1).
    #[inline]
    pub const fn ssip(self) -> bool {
        ((self.bits >> 1) & 1) != 0
    }

    /// Supervisor timer interrupt enable in mvien (bit 5).
    #[inline]
    pub const fn stip(self) -> bool {
        ((self.bits >> 5) & 1) != 0
    }

    /// Supervisor external interrupt enable in mvien (bit 9).
    #[inline]
    pub const fn seip(self) -> bool {
        ((self.bits >> 9) & 1) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mvien_bits() {
        let bits: usize = (1usize << 1) | (1usize << 5) | (1usize << 9);
        let reg = Mvien::from_bits(bits);
        assert!(reg.ssip());
        assert!(reg.stip());
        assert!(reg.seip());
    }

    #[test]
    fn mvien_zero() {
        let reg = Mvien::from_bits(0);
        assert!(!reg.ssip());
        assert!(!reg.stip());
        assert!(!reg.seip());
    }
}
