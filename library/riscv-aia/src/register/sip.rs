//! Supervisor interrupt-pending bits (sip)

riscv::read_write_csr! {
    /// Supervisor interrupt-pending bits.
    Sip: 0x144,
    // 0xFFFF_E222 in RV32, or 0xFFFF_FFFF_FFFF_E222 in RV64
    mask: usize::MAX & !0x1DDD,
}

impl Sip {
    /// Test bit `n` of `sip`.
    #[inline]
    pub const fn bit(self, n: usize) -> bool {
        ((self.bits >> n) & 1) != 0
    }
}

riscv::read_only_csr_field! {
    Sip,
    /// Supervisor software interrupt pending.
    ssip: 1,
}

riscv::read_only_csr_field! {
    Sip,
    /// Supervisor timer interrupt pending.
    stip: 5,
}

riscv::read_only_csr_field! {
    Sip,
    /// Supervisor external interrupt pending.
    seip: 9,
}

riscv::read_only_csr_field! {
    Sip,
    /// Counter overflow interrupt pending.
    counter_overflow: 13,
}

#[cfg(not(target_pointer_width = "32"))]
riscv::read_only_csr_field! {
    Sip,
    /// Low-priority RAS event interrupt pending.
    low_priority_ras_event: 35,
}

#[cfg(not(target_pointer_width = "32"))]
riscv::read_only_csr_field! {
    Sip,
    /// High-priority RAS event interrupt pending.
    high_priority_ras_event: 43,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sip_fields_are_one_hot() {
        let ssip = Sip::from_bits(1 << 1);
        assert!(ssip.ssip());
        assert!(!ssip.stip());
        assert!(!ssip.seip());
        assert!(!ssip.counter_overflow());

        let stip = Sip::from_bits(1 << 5);
        assert!(!stip.ssip());
        assert!(stip.stip());
        assert!(!stip.seip());
        assert!(!stip.counter_overflow());

        let seip = Sip::from_bits(1 << 9);
        assert!(!seip.ssip());
        assert!(!seip.stip());
        assert!(seip.seip());
        assert!(!seip.counter_overflow());

        let counter_overflow = Sip::from_bits(1 << 13);
        assert!(!counter_overflow.ssip());
        assert!(!counter_overflow.stip());
        assert!(!counter_overflow.seip());
        assert!(counter_overflow.counter_overflow());
    }

    #[cfg(not(target_pointer_width = "32"))]
    #[test]
    fn sip_ras_fields_are_one_hot() {
        let low = Sip::from_bits(1usize << 35);
        assert!(low.low_priority_ras_event());
        assert!(!low.high_priority_ras_event());

        let high = Sip::from_bits(1usize << 43);
        assert!(!high.low_priority_ras_event());
        assert!(high.high_priority_ras_event());
    }

    #[test]
    fn sip_mask() {
        let expected = usize::MAX & !0x1DDD;
        assert_eq!(Sip::BITMASK, expected);
        assert_eq!(Sip::from_bits(usize::MAX).bits(), expected);
    }
}
