//! Hypervisor virtual interrupt enables high-half (hvienh) (RV32 only)

riscv::read_write_csr! {
    /// Upper 32 bits of hvien.
    Hvienh: 0x618,
    mask: 0xFFFF_FFFF,
}

impl Hvienh {
    #[inline]
    pub const fn raw(self) -> usize { self.bits }
}
