//! Virtual supervisor interrupt-enable bits (vsie)

riscv::read_write_csr! {
    /// Virtual supervisor interrupt-enable bits.
    Vsie: 0x204,
    // 0xFFFF_E222 in RV32, or 0xFFFF_FFFF_FFFF_E222 in RV64
    mask: usize::MAX & !0x1DDD,
}

impl Vsie {
    /// Tests bit `number` of `vsie` for the current XLEN.
    ///
    /// # Panics
    ///
    /// Panics if `number` is outside the current XLEN.
    #[inline]
    pub const fn bit(self, number: usize) -> bool {
        assert!(number < usize::BITS as usize);
        ((self.bits >> number) & 1) != 0
    }
}

riscv::read_write_csr_field! {
    Vsie,
    /// VS-level software interrupt enable (bit 1 in `vsie`).
    vssip: 1,
}

riscv::read_write_csr_field! {
    Vsie,
    /// VS-level timer interrupt enable (bit 5 in `vsie`).
    vstip: 5,
}

riscv::read_write_csr_field! {
    Vsie,
    /// VS-level external interrupt enable (bit 9 in `vsie`).
    vseip: 9,
}

riscv::read_write_csr_field! {
    Vsie,
    /// Counter overflow interrupt enable.
    counter_overflow: 13,
}

#[cfg(target_pointer_width = "64")]
riscv::read_write_csr_field! {
    Vsie,
    /// Low-priority RAS event interrupt enable.
    low_priority_ras_event: 35,
}

#[cfg(target_pointer_width = "64")]
riscv::read_write_csr_field! {
    Vsie,
    /// High-priority RAS event interrupt enable.
    high_priority_ras_event: 43,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vsie_fields_one_hot() {
        let vssip = Vsie::from_bits(1 << 1);
        assert!(vssip.vssip());
        assert!(!vssip.vstip());
        assert!(!vssip.vseip());
        assert!(!vssip.counter_overflow());

        let vstip = Vsie::from_bits(1 << 5);
        assert!(!vstip.vssip());
        assert!(vstip.vstip());
        assert!(!vstip.vseip());
        assert!(!vstip.counter_overflow());

        let vseip = Vsie::from_bits(1 << 9);
        assert!(!vseip.vssip());
        assert!(!vseip.vstip());
        assert!(vseip.vseip());
        assert!(!vseip.counter_overflow());

        let counter_overflow = Vsie::from_bits(1 << 13);
        assert!(!counter_overflow.vssip());
        assert!(!counter_overflow.vstip());
        assert!(!counter_overflow.vseip());
        assert!(counter_overflow.counter_overflow());
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn vsie_ras_fields_one_hot() {
        let low = Vsie::from_bits(1usize << 35);
        assert!(low.low_priority_ras_event());
        assert!(!low.high_priority_ras_event());

        let high = Vsie::from_bits(1usize << 43);
        assert!(!high.low_priority_ras_event());
        assert!(high.high_priority_ras_event());
    }

    #[test]
    fn vsie_mask() {
        let expected = usize::MAX & !0x1DDD;
        assert_eq!(Vsie::BITMASK, expected);
        assert_eq!(Vsie::from_bits(usize::MAX).bits(), expected);
        assert_eq!(Vsie::from_bits((1 << 2) | (1 << 6) | (1 << 10)).bits(), 0);
    }

    #[test]
    #[should_panic]
    fn vsie_bit_bounds() {
        Vsie::from_bits(0).bit(usize::BITS as usize);
    }
}
