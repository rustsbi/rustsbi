//! Machine interrupt-enable high-half (RV32 only) (mieh)

riscv::read_write_csr! {
    /// Upper 32 bits of mie (RV32 only).
    Mieh: 0x314,
    mask: 0xFFFF_FFFF,
}

impl Mieh {
    #[inline]
    pub const fn raw(self) -> usize { self.bits }
}
