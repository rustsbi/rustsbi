//! Machine interrupt-pending high-half (miph) (RV32 only)

riscv::read_write_csr! {
    /// Upper 32 bits of mip (RV32 only).
    Miph: 0x354,
    mask: 0xFFFF_FFFF,
}

impl Miph {
    #[inline]
    pub const fn raw(self) -> usize { self.bits }
}
