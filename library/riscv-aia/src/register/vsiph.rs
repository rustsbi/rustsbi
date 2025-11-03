//! Virtual supervisor interrupt-pending high-half (vsiph) (RV32 only)

riscv::read_write_csr! {
    /// Upper 32 bits of vsip.
    Vsiph: 0x254,
    mask: 0xFFFF_FFFF,
}
