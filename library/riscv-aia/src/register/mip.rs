//! Machine interrupt-pending bits (mip)

riscv::read_write_csr! {
    /// Machine interrupt-pending bits.
    Mip: 0x344,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}
