//! Supervisor indirect register select (siselect)

riscv::read_write_csr! {
    /// Supervisor indirect register select.
    Siselect: 0x150,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}
