//! Hypervisor interrupt delegation high-half (hidelegh) (RV32 only)

riscv::read_write_csr! {
    /// Upper 32 bits of hideleg.
    Hidelegh: 0x613,
    mask: 0xFFFF_FFFF,
}
