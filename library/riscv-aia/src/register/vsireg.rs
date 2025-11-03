//! Virtual supervisor indirect register alias (vsireg)

riscv::read_write_csr! {
    /// Virtual supervisor indirect register alias.
    Vsireg: 0x251,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}
