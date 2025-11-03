//! Hypervisor interrupt delegation (hideleg)

riscv::read_write_csr! {
    /// Hypervisor interrupt delegation.
    Hideleg: 0x603,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}
