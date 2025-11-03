//! Supervisor interrupt-enable high-half (sieh) (RV32 only)

riscv::read_write_csr! {
    /// Upper 32 bits of sie (RV32 only).
    Sieh: 0x114,
    mask: 0xFFFF_FFFF,
}
