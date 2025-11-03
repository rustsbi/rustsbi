//! Hypervisor VIPRIO2 high-half (hviprio2h) (RV32 only)

riscv::read_write_csr! {
    /// Upper 32 bits of hviprio2.
    Hviprio2h: 0x657,
    mask: 0xFFFF_FFFF,
}
