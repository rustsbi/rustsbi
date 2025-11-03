//! Supervisor indirect register alias (sireg)

riscv::read_write_csr! {
    /// Supervisor indirect register alias.
    Sireg: 0x151,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}

impl Sireg {
    /// Raw bits read from `sireg` (width depends on XLEN).
    #[inline]
    pub const fn raw(self) -> usize { self.bits }

    /// Convenience accessor returning bits as usize.
    #[inline]
    pub const fn as_usize(self) -> usize { self.bits }
}
