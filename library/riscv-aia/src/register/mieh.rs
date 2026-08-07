//! Machine interrupt-enable high-half (mieh) (RV32 only)

riscv::csr! {
    /// Upper 32 bits of `mie` (RV32 only).
    Mieh,
    0xFFFF_FFFF
}
riscv::read_csr_as_rv32!(Mieh, 0x314);
riscv::write_csr_as_rv32!(Mieh, 0x314);

riscv::read_write_csr_field! {
    Mieh,
    /// Low-priority RAS event interrupt enable (interrupt 35).
    low_priority_ras_event: 3, // 35 - 32
}

riscv::read_write_csr_field! {
    Mieh,
    /// High-priority RAS event interrupt enable (interrupt 43).
    high_priority_ras_event: 11, // 43 - 32
}

riscv::set_rv32!(0x314);
riscv::clear_rv32!(0x314);

riscv::set_clear_csr!(
    /// Low-priority RAS event interrupt enable.
    , set_low_priority_ras_event, clear_low_priority_ras_event, 1 << 3);
riscv::set_clear_csr!(
    /// High-priority RAS event interrupt enable.
    , set_high_priority_ras_event, clear_high_priority_ras_event, 1 << 11);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mieh_ras_fields() {
        let low = Mieh::from_bits(1 << 3);
        assert!(low.low_priority_ras_event());
        assert!(!low.high_priority_ras_event());

        let high = Mieh::from_bits(1 << 11);
        assert!(!high.low_priority_ras_event());
        assert!(high.high_priority_ras_event());
    }

    #[test]
    fn mieh_mask() {
        let reg = Mieh::from_bits(0x1234_5678);
        assert_eq!(Mieh::BITMASK, 0xFFFF_FFFF);
        assert_eq!(reg.bits(), 0x1234_5678);
    }
}
