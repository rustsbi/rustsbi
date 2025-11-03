//! Virtual supervisor interrupt-pending bits (vsip)

riscv::read_write_csr! {
    /// Virtual supervisor interrupt-pending bits.
    Vsip: 0x244,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}
