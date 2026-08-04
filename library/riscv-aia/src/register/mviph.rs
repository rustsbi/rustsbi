//! Machine virtual interrupt-pending high half (`mviph`) (RV32 only).

riscv::read_write_csr! {
    /// Upper 32 bits of `mvip` (RV32 only).
    Mviph: 0x319,
    mask: 0xFFFF_FFFF,
}

riscv::read_only_csr_field! {
    Mviph,
    /// Low-priority RAS event interrupt pending (interrupt 35).
    low_priority_ras_event: 3, // 35 - 32
}

riscv::read_only_csr_field! {
    Mviph,
    /// High-priority RAS event interrupt pending (interrupt 43).
    high_priority_ras_event: 11, // 43 - 32
}

riscv::set!(0x319);
riscv::clear!(0x319);

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
    fn mviph_ras_fields_are_one_hot() {
        let low = Mviph::from_bits(1 << 3);
        assert!(low.low_priority_ras_event());
        assert!(!low.high_priority_ras_event());

        let high = Mviph::from_bits(1 << 11);
        assert!(!high.low_priority_ras_event());
        assert!(high.high_priority_ras_event());
    }

    #[test]
    fn mviph_mask() {
        let bits = 0x0F0F_0F0F;
        let reg = Mviph::from_bits(bits);
        assert_eq!(Mviph::BITMASK, 0xFFFF_FFFF);
        assert_eq!(reg.bits(), bits);
    }
}
