//! Virtual supervisor interrupt-pending high-half (vsiph) (RV32 only)

riscv::csr! {
    /// Upper 32 bits of vsip.
    Vsiph,
    0xFFFF_FFFF
}
riscv::read_csr_as_rv32!(Vsiph, 0x254);
riscv::write_csr_as_rv32!(Vsiph, 0x254);

riscv::read_only_csr_field! {
    Vsiph,
    /// Low-priority RAS event interrupt pending (interrupt 35).
    low_priority_ras_event: 3,
}

riscv::read_only_csr_field! {
    Vsiph,
    /// High-priority RAS event interrupt pending (interrupt 43).
    high_priority_ras_event: 11,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vsiph_ras_fields_one_hot() {
        let low = Vsiph::from_bits(1 << 3);
        assert!(low.low_priority_ras_event());
        assert!(!low.high_priority_ras_event());

        let high = Vsiph::from_bits(1 << 11);
        assert!(!high.low_priority_ras_event());
        assert!(high.high_priority_ras_event());
    }

    #[test]
    fn vsiph_mask() {
        assert_eq!(Vsiph::BITMASK, 0xFFFF_FFFF);
        assert_eq!(Vsiph::from_bits(usize::MAX).bits(), 0xFFFF_FFFF);
    }
}
