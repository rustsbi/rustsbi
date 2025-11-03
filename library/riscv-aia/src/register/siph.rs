//! Supervisor interrupt-pending high-half (siph) (RV32 only)

riscv::read_write_csr! {
    /// Upper 32 bits of sip (RV32 only).
    Siph: 0x154,
    mask: 0xFFFF_FFFF,
}
