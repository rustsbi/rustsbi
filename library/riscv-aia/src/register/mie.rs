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
    pub const fn ssip(self) -> bool { self.bit(1) }

    /// Supervisor timer interrupt enable (bit 5).
    #[inline]
    pub const fn stip(self) -> bool { self.bit(5) }

    /// Supervisor external interrupt enable (bit 9).
    #[inline]
    pub const fn seip(self) -> bool { self.bit(9) }

    /// Machine software interrupt enable (bit 3).
    #[inline]
    pub const fn msip(self) -> bool { self.bit(3) }

    /// Machine timer interrupt enable (bit 7).
    #[inline]
    pub const fn mtip(self) -> bool { self.bit(7) }

    /// Machine external interrupt enable (bit 11).
    #[inline]
    pub const fn meip(self) -> bool { self.bit(11) }
}
