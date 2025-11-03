//! Machine virtual interrupt pending bits (mvip)

riscv::read_write_csr! {
    /// Machine virtual interrupt pending bits.
    Mvip: 0x309,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}
