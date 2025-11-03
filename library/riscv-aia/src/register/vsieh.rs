//! Virtual supervisor interrupt-enable high-half (vsieh) (RV32 only)

riscv::read_write_csr! {
    /// Upper 32 bits of vsie.
    Vsieh: 0x214,
    mask: 0xFFFF_FFFF,
}
