//! Virtual supervisor indirect register alias (vsireg)

riscv::read_write_csr! {
    /// Virtual supervisor indirect register alias.
    Vsireg: 0x251,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}

impl Vsireg {
    /// Raw bits read from `vsireg` (width depends on XLEN).
    #[inline]
    pub const fn raw(self) -> usize { self.bits }

    /// Convenience accessor returning bits as usize.
    #[inline]
    pub const fn as_usize(self) -> usize { self.bits }
}
