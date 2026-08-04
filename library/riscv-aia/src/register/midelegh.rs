//! Machine-level interrupt delegation register, high 32-bit part (RV32 only).

riscv::read_write_csr! {
    /// Machine-level interrupt delegation register, high 32-bit part (RV32 only).
    Midelegh: 0x313,
    mask: 0xFFFF_FFFF,
}

riscv::read_write_csr_field! {
    Midelegh,
    /// Low-priority RAS event interrupt delegation.
    low_priority_ras_event: 3, // 35 - 32
}

riscv::read_write_csr_field! {
    Midelegh,
    /// High-priority RAS event interrupt delegation.
    high_priority_ras_event: 11, // 43 - 32
}

riscv::set!(0x313);
riscv::clear!(0x313);

riscv::set_clear_csr!(
    /// Low-priority RAS event interrupt delegation.
    , set_low_priority_ras_event, clear_low_priority_ras_event, 1 << 3);
riscv::set_clear_csr!(
    /// High-priority RAS event interrupt delegation.
    , set_high_priority_ras_event, clear_high_priority_ras_event, 1 << 11);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn midelegh_ras_fields() {
        let low = Midelegh::from_bits(1 << 3);
        assert!(low.low_priority_ras_event());
        assert!(!low.high_priority_ras_event());

        let high = Midelegh::from_bits(1 << 11);
        assert!(!high.low_priority_ras_event());
        assert!(high.high_priority_ras_event());
    }

    #[test]
    fn midelegh_mask() {
        let reg = Midelegh::from_bits(0x1234_5678);
        assert_eq!(Midelegh::BITMASK, 0xFFFF_FFFF);
        assert_eq!(reg.bits(), 0x1234_5678);
    }
}
