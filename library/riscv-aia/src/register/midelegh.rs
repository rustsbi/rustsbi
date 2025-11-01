//! Machine-level interrupt delegation register, high 32-bit part (RV32 only).

riscv::read_write_csr! {
    /// Machine-level interrupt delegation register, high 32-bit part (RV32 only).
    Midelegh: 0x313,
    mask: 0x808,
}

riscv::read_write_csr_field! {
    Midelegh,
    /// Low priority RAS event interrupt delegation.
    low_priority_ras_event: 3, // 35 - 32
}

riscv::read_write_csr_field! {
    Midelegh,
    /// High priority RAS event interrupt delegation.
    high_priority_ras_event: 11, // 43 - 32
}

// TODO mod tests
