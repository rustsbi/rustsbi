//! Hypervisor virtual interrupt pending high-half (hviph) (RV32 only)

riscv::csr! {
    /// Upper 32 bits of hvip.
    Hviph,
    0xFFFF_FFFF
}
riscv::read_csr_as_rv32!(Hviph, 0x655);
riscv::write_csr_as_rv32!(Hviph, 0x655);

riscv::read_only_csr_field! {
    Hviph,
    /// Low-priority RAS event interrupt pending (interrupt 35).
    low_priority_ras_event: 3, // 35 - 32
}

riscv::read_only_csr_field! {
    Hviph,
    /// High-priority RAS event interrupt pending (interrupt 43).
    high_priority_ras_event: 11, // 43 - 32
}

riscv::set_rv32!(0x655);
riscv::clear_rv32!(0x655);

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
    fn hviph_raw_roundtrip() {
        let bits: usize = 0xDEAD_BEEFusize & 0xFFFF_FFFF;
        let p = Hviph::from_bits(bits);
        assert_eq!(p.bits(), bits);
    }

    #[test]
    fn hviph_ras_fields() {
        let low = Hviph::from_bits(1 << 3);
        assert!(low.low_priority_ras_event());
        assert!(!low.high_priority_ras_event());

        let high = Hviph::from_bits(1 << 11);
        assert!(!high.low_priority_ras_event());
        assert!(high.high_priority_ras_event());
    }
}
