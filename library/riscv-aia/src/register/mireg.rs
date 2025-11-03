//! Machine indirect register alias (mireg)

riscv::read_write_csr! {
    /// Machine indirect register alias.
    Mireg: 0x351,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}
