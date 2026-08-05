//! Hypervisor virtual interrupt enables high-half (hvienh) (RV32 only)

riscv::read_write_csr! {
    /// Upper 32 bits of hvien.
    Hvienh: 0x618,
    mask: 0xFFFF_FFFF,
}

riscv::read_write_csr_field! {
    Hvienh,
    /// Low-priority RAS event interrupt virtual enable (interrupt 35).
    low_priority_ras_event: 3,
}

riscv::read_write_csr_field! {
    Hvienh,
    /// High-priority RAS event interrupt virtual enable (interrupt 43).
    high_priority_ras_event: 11,
}

riscv::set!(0x618);
riscv::clear!(0x618);

riscv::set_clear_csr!(
    /// Low-priority RAS event interrupt virtual enable.
    , set_low_priority_ras_event, clear_low_priority_ras_event, 1 << 3);
riscv::set_clear_csr!(
    /// High-priority RAS event interrupt virtual enable.
    , set_high_priority_ras_event, clear_high_priority_ras_event, 1 << 11);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hvienh_ras_fields_are_one_hot() {
        let low = Hvienh::from_bits(1 << 3);
        assert!(low.low_priority_ras_event());
        assert!(!low.high_priority_ras_event());

        let high = Hvienh::from_bits(1 << 11);
        assert!(!high.low_priority_ras_event());
        assert!(high.high_priority_ras_event());
    }

    #[test]
    fn hvienh_mask() {
        assert_eq!(Hvienh::BITMASK, 0xFFFF_FFFF);
        assert_eq!(Hvienh::from_bits(usize::MAX).bits(), 0xFFFF_FFFF);
    }
}
