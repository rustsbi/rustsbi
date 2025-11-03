//! Virtual supervisor interrupt-pending bits (vsip)

riscv::read_write_csr! {
    /// Virtual supervisor interrupt-pending bits.
    Vsip: 0x244,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}

impl Vsip {
    /// Test bit `n` of `vsip`.
    #[inline]
    pub const fn bit(self, n: usize) -> bool {
        ((self.bits >> n) & 1) != 0
    }

    /// VS-level software interrupt pending (bit 2).
    #[inline]
    pub const fn vssip(self) -> bool { self.bit(2) }

    /// VS-level timer interrupt pending (bit 6).
    #[inline]
    pub const fn vstip(self) -> bool { self.bit(6) }

    /// VS-level external interrupt pending (bit 10).
    #[inline]
    pub const fn vseip(self) -> bool { self.bit(10) }
}
