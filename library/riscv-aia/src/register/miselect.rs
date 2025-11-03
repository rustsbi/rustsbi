//! Machine indirect register select (miselect)

riscv::read_write_csr! {
    /// Machine indirect register select.
    Miselect: 0x350,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}

// Note: miselect controls which register is accessed via `mireg`.

impl Miselect {
    /// Current value of `miselect` as usize (convenience accessor).
    #[inline]
    pub const fn value(self) -> usize {
        self.bits as usize
    }

    // Note: writing to `miselect` should be done via the generated CSR API.
}
