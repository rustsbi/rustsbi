//! Hypervisor VIPRIO1 high-half (hviprio1h) (RV32 only)

riscv::read_write_csr! {
    /// Upper 32 bits of hviprio1.
    Hviprio1h: 0x656,
    mask: 0xFFFF_FFFF,
}

impl Hviprio1h {
    #[inline]
    pub const fn raw(self) -> usize { self.bits }
}
