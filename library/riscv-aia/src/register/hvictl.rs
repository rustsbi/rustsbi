//! Hypervisor virtual interrupt control (hvictl)

riscv::read_write_csr! {
    /// Hypervisor virtual interrupt control.
    Hvictl: 0x609,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}
