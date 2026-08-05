//! Supervisor interrupt-pending high-half (siph) (RV32 only)

riscv::read_write_csr! {
    /// Upper 32 bits of sip (RV32 only).
    Siph: 0x154,
    mask: 0xFFFF_FFFF,
}

riscv::read_only_csr_field! {
    Siph,
    /// Low-priority RAS event interrupt pending (interrupt 35).
    low_priority_ras_event: 3,
}

riscv::read_only_csr_field! {
    Siph,
    /// High-priority RAS event interrupt pending (interrupt 43).
    high_priority_ras_event: 11,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn siph_ras_fields_are_one_hot() {
        let low = Siph::from_bits(1 << 3);
        assert!(low.low_priority_ras_event());
        assert!(!low.high_priority_ras_event());

        let high = Siph::from_bits(1 << 11);
        assert!(!high.low_priority_ras_event());
        assert!(high.high_priority_ras_event());
    }

    #[test]
    fn siph_mask() {
        assert_eq!(Siph::BITMASK, 0xFFFF_FFFF);
        assert_eq!(Siph::from_bits(usize::MAX).bits(), 0xFFFF_FFFF);
    }
}
