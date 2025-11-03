//! Machine interrupt delegation (mideleg)

riscv::read_write_csr! {
    /// Machine interrupt delegation.
    Mideleg: 0x303,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}

impl Mideleg {
    /// Test bit `n` of `mideleg` (generic helper).
    #[inline]
    pub const fn bit(self, n: usize) -> bool {
        ((self.bits >> n) & 1) != 0
    }

    /// Supervisor software interrupt delegation (bit 1).
    #[inline]
    pub const fn ssip(self) -> bool { self.bit(1) }

    /// Supervisor timer interrupt delegation (bit 5).
    #[inline]
    pub const fn stip(self) -> bool { self.bit(5) }

    /// Supervisor external interrupt delegation (bit 9).
    #[inline]
    pub const fn seip(self) -> bool { self.bit(9) }
}
