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

riscv::set!(0x313);
riscv::clear!(0x313);

riscv::set_clear_csr!(
    /// Virtual Supervisor Timer Interrupt delegation.
    , set_low_priority_ras_event, clear_low_priority_ras_event, 1 << 3);
riscv::set_clear_csr!(
    /// High priority RAS event interrupt delegation.
    , set_high_priority_ras_event, clear_high_priority_ras_event, 1 << 11);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn midelegh_fields_bits() {
        // low_priority_ras_event is at bit 3, high_priority_ras_event at bit 11
        let bits: usize = 0x808;
        let reg = Midelegh::from_bits(bits);
        // The macro-generated accessors are expected to be named after the fields.
        assert!(reg.low_priority_ras_event());
        assert!(reg.high_priority_ras_event());
        // unset a bit
        let reg2 = Midelegh::from_bits(0);
        assert!(!reg2.low_priority_ras_event());
        assert!(!reg2.high_priority_ras_event());
    }
}
