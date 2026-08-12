//! Supervisor interrupt-enable bits (sie)

riscv::read_write_csr! {
    /// Supervisor interrupt-enable bits.
    Sie: 0x104,
    // 0xFFFF_E222 in RV32, or 0xFFFF_FFFF_FFFF_E222 in RV64
    mask: usize::MAX & !0x1DDD,
}

riscv::read_write_csr_field! {
    Sie,
    /// Supervisor software interrupt enable.
    ssip: 1,
}

riscv::read_write_csr_field! {
    Sie,
    /// Supervisor timer interrupt enable.
    stip: 5,
}

riscv::read_write_csr_field! {
    Sie,
    /// Supervisor external interrupt enable.
    seip: 9,
}

riscv::read_write_csr_field! {
    Sie,
    /// Counter overflow interrupt enable.
    counter_overflow: 13,
}

#[cfg(target_pointer_width = "64")]
riscv::read_write_csr_field! {
    Sie,
    /// Low-priority RAS event interrupt enable.
    low_priority_ras_event: 35,
}

#[cfg(target_pointer_width = "64")]
riscv::read_write_csr_field! {
    Sie,
    /// High-priority RAS event interrupt enable.
    high_priority_ras_event: 43,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sie_fields_one_hot() {
        let ssip = Sie::from_bits(1 << 1);
        assert!(ssip.ssip());
        assert!(!ssip.stip());
        assert!(!ssip.seip());
        assert!(!ssip.counter_overflow());

        let stip = Sie::from_bits(1 << 5);
        assert!(!stip.ssip());
        assert!(stip.stip());
        assert!(!stip.seip());
        assert!(!stip.counter_overflow());

        let seip = Sie::from_bits(1 << 9);
        assert!(!seip.ssip());
        assert!(!seip.stip());
        assert!(seip.seip());
        assert!(!seip.counter_overflow());

        let counter_overflow = Sie::from_bits(1 << 13);
        assert!(!counter_overflow.ssip());
        assert!(!counter_overflow.stip());
        assert!(!counter_overflow.seip());
        assert!(counter_overflow.counter_overflow());
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn sie_ras_fields_one_hot() {
        let low = Sie::from_bits(1usize << 35);
        assert!(low.low_priority_ras_event());
        assert!(!low.high_priority_ras_event());

        let high = Sie::from_bits(1usize << 43);
        assert!(!high.low_priority_ras_event());
        assert!(high.high_priority_ras_event());
    }

    #[test]
    fn sie_mask() {
        let expected = usize::MAX & !0x1DDD;
        assert_eq!(Sie::BITMASK, expected);
        assert_eq!(Sie::from_bits(usize::MAX).bits(), expected);
    }
}
