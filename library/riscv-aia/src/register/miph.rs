//! Machine interrupt-pending high-half (miph) (RV32 only)

riscv::csr! {
    /// Upper 32 bits of `mip` (RV32 only).
    Miph,
    0xFFFF_FFFF
}
riscv::read_csr_as_rv32!(Miph, 0x354);
riscv::write_csr_as_rv32!(Miph, 0x354);

riscv::read_only_csr_field! {
    Miph,
    /// Low-priority RAS event interrupt pending (interrupt 35).
    low_priority_ras_event: 3, // 35 - 32
}

riscv::read_only_csr_field! {
    Miph,
    /// High-priority RAS event interrupt pending (interrupt 43).
    high_priority_ras_event: 11, // 43 - 32
}

riscv::set_rv32!(0x354);
riscv::clear_rv32!(0x354);

riscv::set_clear_csr!(
    /// Low-priority RAS event interrupt pending.
    , set_low_priority_ras_event, clear_low_priority_ras_event, 1 << 3);
riscv::set_clear_csr!(
    /// High-priority RAS event interrupt pending.
    , set_high_priority_ras_event, clear_high_priority_ras_event, 1 << 11);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn miph_ras_fields_one_hot() {
        let low = Miph::from_bits(1 << 3);
        assert!(low.low_priority_ras_event());
        assert!(!low.high_priority_ras_event());

        let high = Miph::from_bits(1 << 11);
        assert!(!high.low_priority_ras_event());
        assert!(high.high_priority_ras_event());
    }

    #[test]
    fn miph_mask() {
        let reg = Miph::from_bits(0x1234_5678);
        assert_eq!(Miph::BITMASK, 0xFFFF_FFFF);
        assert_eq!(reg.bits(), 0x1234_5678);
    }
}
