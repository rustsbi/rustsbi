//! Supervisor interrupt-enable high-half (sieh) (RV32 only)

riscv::read_write_csr! {
    /// Upper 32 bits of sie (RV32 only).
    Sieh: 0x114,
    mask: 0xFFFF_FFFF,
}

riscv::read_write_csr_field! {
    Sieh,
    /// Low-priority RAS event interrupt enable (interrupt 35).
    low_priority_ras_event: 3,
}

riscv::read_write_csr_field! {
    Sieh,
    /// High-priority RAS event interrupt enable (interrupt 43).
    high_priority_ras_event: 11,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sieh_ras_fields_are_one_hot() {
        let low = Sieh::from_bits(1 << 3);
        assert!(low.low_priority_ras_event());
        assert!(!low.high_priority_ras_event());

        let high = Sieh::from_bits(1 << 11);
        assert!(!high.low_priority_ras_event());
        assert!(high.high_priority_ras_event());
    }

    #[test]
    fn sieh_mask() {
        assert_eq!(Sieh::BITMASK, 0xFFFF_FFFF);
        assert_eq!(Sieh::from_bits(usize::MAX).bits(), 0xFFFF_FFFF);
    }
}
