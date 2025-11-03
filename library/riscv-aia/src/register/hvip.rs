//! Hypervisor virtual interrupt pending bits (hvip)

riscv::read_write_csr! {
    /// Hypervisor virtual interrupt pending bits.
    Hvip: 0x645,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}

impl Hvip {
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
