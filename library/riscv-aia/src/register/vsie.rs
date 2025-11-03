//! Virtual supervisor interrupt-enable bits (vsie)

riscv::read_write_csr! {
    /// Virtual supervisor interrupt-enable bits.
    Vsie: 0x204,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}
