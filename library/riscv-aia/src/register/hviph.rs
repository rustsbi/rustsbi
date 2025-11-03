//! Hypervisor virtual interrupt pending high-half (hviph) (RV32 only)

riscv::read_write_csr! {
    /// Upper 32 bits of hvip.
    Hviph: 0x655,
    mask: 0xFFFF_FFFF,
}
