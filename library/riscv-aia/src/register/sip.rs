//! Supervisor interrupt-pending bits (sip)

riscv::read_write_csr! {
    /// Supervisor interrupt-pending bits.
    Sip: 0x144,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}
