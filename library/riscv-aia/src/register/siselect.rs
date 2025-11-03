//! Supervisor indirect register select (siselect)

riscv::read_write_csr! {
    /// Supervisor indirect register select.
    Siselect: 0x150,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}

impl Siselect {
    /// Current value of `siselect` as usize.
    #[inline]
    pub const fn value(self) -> usize { self.bits as usize }

    // Note: writing to `siselect` should be done via the generated CSR API.
}
