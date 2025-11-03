//! Machine virtual interrupt enables (mvien)

riscv::read_write_csr! {
    /// Machine virtual interrupt enables.
    Mvien: 0x308,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}
