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
    pub const fn vssip(self) -> bool { self.bit(2) }

    /// VS-level timer interrupt enable (bit 6).
    #[inline]
    pub const fn vstip(self) -> bool { self.bit(6) }

    /// VS-level external interrupt enable (bit 10).
    #[inline]
    pub const fn vseip(self) -> bool { self.bit(10) }
}
