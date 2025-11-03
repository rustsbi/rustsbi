//! Hypervisor VS-level interrupt priority 1 (hviprio1)

riscv::read_write_csr! {
    /// Hypervisor VS-level interrupt priority 1.
    Hviprio1: 0x646,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}
