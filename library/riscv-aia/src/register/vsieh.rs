//! Virtual supervisor interrupt-enable high-half (vsieh) (RV32 only)

riscv::csr! {
    /// Upper 32 bits of vsie.
    Vsieh,
    0xFFFF_FFFF
}
riscv::read_csr_as_rv32!(Vsieh, 0x214);
riscv::write_csr_as_rv32!(Vsieh, 0x214);

riscv::read_write_csr_field! {
    Vsieh,
    /// Low-priority RAS event interrupt enable (interrupt 35).
    low_priority_ras_event: 3,
}

riscv::read_write_csr_field! {
    Vsieh,
    /// High-priority RAS event interrupt enable (interrupt 43).
    high_priority_ras_event: 11,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vsieh_ras_fields_are_one_hot() {
        let low = Vsieh::from_bits(1 << 3);
        assert!(low.low_priority_ras_event());
        assert!(!low.high_priority_ras_event());

        let high = Vsieh::from_bits(1 << 11);
        assert!(!high.low_priority_ras_event());
        assert!(high.high_priority_ras_event());
    }

    #[test]
    fn vsieh_mask() {
        assert_eq!(Vsieh::BITMASK, 0xFFFF_FFFF);
        assert_eq!(Vsieh::from_bits(usize::MAX).bits(), 0xFFFF_FFFF);
    }
}
