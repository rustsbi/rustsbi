//! Machine interrupt-enable bits (mie)

riscv::read_write_csr! {
    /// Machine interrupt-enable bits.
    Mie: 0x304,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}
