//! Hypervisor VS-level interrupt priority 2 (hviprio2)

riscv::read_write_csr! {
    /// Hypervisor VS-level interrupt priority 2.
    Hviprio2: 0x647,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}
