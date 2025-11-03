//! Machine interrupt delegation (mideleg)

riscv::read_write_csr! {
    /// Machine interrupt delegation.
    Mideleg: 0x303,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}
