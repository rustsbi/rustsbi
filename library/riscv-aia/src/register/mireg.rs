//! Machine indirect register alias (mireg)

riscv::read_write_csr! {
    /// Machine indirect register alias.
    Mireg: 0x351,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}

impl Mireg {
    /// Raw bits read from `mireg` (convenience accessors - width depends on XLEN).
    #[inline]
    pub const fn raw(self) -> usize { self.bits }

    /// Raw bits as usize convenience accessor.
    #[inline]
    pub const fn as_usize(self) -> usize { self.bits }
}
