//! Hypervisor virtual interrupt enables (hvien)

riscv::read_write_csr! {
    /// Hypervisor virtual interrupt enables.
    Hvien: 0x608,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}
