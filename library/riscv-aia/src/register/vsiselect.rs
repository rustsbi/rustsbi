//! Virtual supervisor indirect register select (vsiselect)

riscv::read_write_csr! {
    /// Virtual supervisor indirect register select.
    Vsiselect: 0x250,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}

impl Vsiselect {
    /// Current value of `vsiselect` as usize.
    #[inline]
    pub const fn value(self) -> usize { self.bits as usize }

    // Note: writing to `vsiselect` should be done via the generated CSR API.
}
