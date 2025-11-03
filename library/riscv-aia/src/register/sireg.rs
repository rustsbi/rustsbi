//! Supervisor indirect register alias (sireg)

riscv::read_write_csr! {
    /// Supervisor indirect register alias.
    Sireg: 0x151,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}
