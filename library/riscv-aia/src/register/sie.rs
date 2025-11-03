//! Supervisor interrupt-enable bits (sie)

riscv::read_write_csr! {
    /// Supervisor interrupt-enable bits.
    Sie: 0x104,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}
