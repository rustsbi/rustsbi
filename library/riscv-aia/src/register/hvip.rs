//! Hypervisor virtual interrupt pending bits (hvip)

riscv::read_write_csr! {
    /// Hypervisor virtual interrupt pending bits.
    Hvip: 0x645,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}
