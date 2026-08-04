//! Machine virtual interrupt enables high half (`mvienh`) (RV32 only).

riscv::read_write_csr! {
    /// Upper 32 bits of `mvien` (RV32 only).
    Mvienh: 0x318,
    mask: 0xFFFF_FFFF,
}

riscv::read_write_csr_field! {
    Mvienh,
    /// Low-priority RAS event interrupt virtual enable (interrupt 35).
    low_priority_ras_event: 3, // 35 - 32
}

riscv::read_write_csr_field! {
    Mvienh,
    /// High-priority RAS event interrupt virtual enable (interrupt 43).
    high_priority_ras_event: 11, // 43 - 32
}

riscv::set!(0x318);
riscv::clear!(0x318);

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
    fn mvienh_ras_fields_are_one_hot() {
        let low = Mvienh::from_bits(1 << 3);
        assert!(low.low_priority_ras_event());
        assert!(!low.high_priority_ras_event());

        let high = Mvienh::from_bits(1 << 11);
        assert!(!high.low_priority_ras_event());
        assert!(high.high_priority_ras_event());
    }

    #[test]
    fn mvienh_mask() {
        let bits = 0xCAFE_BABE;
        let reg = Mvienh::from_bits(bits);
        assert_eq!(Mvienh::BITMASK, 0xFFFF_FFFF);
        assert_eq!(reg.bits(), bits);
    }
}
