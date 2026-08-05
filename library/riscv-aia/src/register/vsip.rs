//! Virtual supervisor interrupt-pending bits (vsip)

riscv::read_write_csr! {
    /// Virtual supervisor interrupt-pending bits.
    Vsip: 0x244,
    // 0xFFFF_E222 in RV32, or 0xFFFF_FFFF_FFFF_E222 in RV64
    mask: usize::MAX & !0x1DDD,
}

impl Vsip {
    /// Test bit `n` of `vsip`.
    #[inline]
    pub const fn bit(self, n: usize) -> bool {
        ((self.bits >> n) & 1) != 0
    }
}

riscv::read_only_csr_field! {
    Vsip,
    /// VS-level software interrupt pending (bit 1 in `vsip`).
    vssip: 1,
}

riscv::read_only_csr_field! {
    Vsip,
    /// VS-level timer interrupt pending (bit 5 in `vsip`).
    vstip: 5,
}

riscv::read_only_csr_field! {
    Vsip,
    /// VS-level external interrupt pending (bit 9 in `vsip`).
    vseip: 9,
}

riscv::read_only_csr_field! {
    Vsip,
    /// Counter overflow interrupt pending.
    counter_overflow: 13,
}

#[cfg(not(target_pointer_width = "32"))]
riscv::read_only_csr_field! {
    Vsip,
    /// Low-priority RAS event interrupt pending.
    low_priority_ras_event: 35,
}

#[cfg(not(target_pointer_width = "32"))]
riscv::read_only_csr_field! {
    Vsip,
    /// High-priority RAS event interrupt pending.
    high_priority_ras_event: 43,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vsip_fields_are_one_hot() {
        let vssip = Vsip::from_bits(1 << 1);
        assert!(vssip.vssip());
        assert!(!vssip.vstip());
        assert!(!vssip.vseip());
        assert!(!vssip.counter_overflow());

        let vstip = Vsip::from_bits(1 << 5);
        assert!(!vstip.vssip());
        assert!(vstip.vstip());
        assert!(!vstip.vseip());
        assert!(!vstip.counter_overflow());

        let vseip = Vsip::from_bits(1 << 9);
        assert!(!vseip.vssip());
        assert!(!vseip.vstip());
        assert!(vseip.vseip());
        assert!(!vseip.counter_overflow());

        let counter_overflow = Vsip::from_bits(1 << 13);
        assert!(!counter_overflow.vssip());
        assert!(!counter_overflow.vstip());
        assert!(!counter_overflow.vseip());
        assert!(counter_overflow.counter_overflow());
    }

    #[cfg(not(target_pointer_width = "32"))]
    #[test]
    fn vsip_ras_fields_are_one_hot() {
        let low = Vsip::from_bits(1usize << 35);
        assert!(low.low_priority_ras_event());
        assert!(!low.high_priority_ras_event());

        let high = Vsip::from_bits(1usize << 43);
        assert!(!high.low_priority_ras_event());
        assert!(high.high_priority_ras_event());
    }

    #[test]
    fn vsip_mask() {
        let expected = usize::MAX & !0x1DDD;
        assert_eq!(Vsip::BITMASK, expected);
        assert_eq!(Vsip::from_bits(usize::MAX).bits(), expected);
        assert_eq!(Vsip::from_bits((1 << 2) | (1 << 6) | (1 << 10)).bits(), 0);
    }
}
