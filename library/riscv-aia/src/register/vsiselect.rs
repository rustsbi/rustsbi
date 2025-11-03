//! Virtual supervisor indirect register select (vsiselect)

riscv::read_write_csr! {
    /// Virtual supervisor indirect register select.
    Vsiselect: 0x250,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}
