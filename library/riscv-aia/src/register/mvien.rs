//! Machine virtual interrupt enables (`mvien`).

riscv::read_write_csr! {
    /// Machine virtual interrupt enables.
    Mvien: 0x308,
    // 0xFFFF_E202 in RV32, or 0xFFFF_FFFF_FFFF_E202 in RV64.
    // Among bits 12:0, only bits 1 and 9 are defined.
    mask: usize::MAX & !0x1DFD,
}

riscv::read_write_csr_field! {
    Mvien,
    /// Supervisor software interrupt virtual enable.
    ssoft: 1,
}

riscv::read_write_csr_field! {
    Mvien,
    /// Supervisor external interrupt virtual enable.
    sext: 9,
}

riscv::read_write_csr_field! {
    Mvien,
    /// Counter overflow interrupt virtual enable.
    counter_overflow: 13,
}

#[cfg(target_pointer_width = "64")]
riscv::read_write_csr_field! {
    Mvien,
    /// Low-priority RAS event interrupt virtual enable.
    low_priority_ras_event: 35,
}

#[cfg(target_pointer_width = "64")]
riscv::read_write_csr_field! {
    Mvien,
    /// High-priority RAS event interrupt virtual enable.
    high_priority_ras_event: 43,
}

impl Mvien {
    /// Returns whether virtual interrupt `number` is enabled.
    ///
    /// `number` must be less than the current XLEN. On RV32, use `mvienh`
    /// for interrupt numbers 32-63.
    #[inline]
    pub const fn bit(self, number: usize) -> bool {
        assert!(number < usize::BITS as usize);
        ((self.bits >> number) & 1) != 0
    }
}

riscv::set!(0x308);
riscv::clear!(0x308);

riscv::set_clear_csr!(
    /// Supervisor software interrupt virtual enable.
    , set_ssoft, clear_ssoft, 1 << 1);
riscv::set_clear_csr!(
    /// Supervisor external interrupt virtual enable.
    , set_sext, clear_sext, 1 << 9);
riscv::set_clear_csr!(
    /// Counter overflow interrupt virtual enable.
    , set_counter_overflow, clear_counter_overflow, 1 << 13);

#[cfg(target_pointer_width = "64")]
riscv::set_clear_csr!(
    /// Low-priority RAS event interrupt virtual enable.
    , set_low_priority_ras_event, clear_low_priority_ras_event, 1usize << 35);
#[cfg(target_pointer_width = "64")]
riscv::set_clear_csr!(
    /// High-priority RAS event interrupt virtual enable.
    , set_high_priority_ras_event, clear_high_priority_ras_event, 1usize << 43);

#[cfg(target_pointer_width = "32")]
pub use super::mvienh::{
    clear_high_priority_ras_event, clear_low_priority_ras_event, set_high_priority_ras_event,
    set_low_priority_ras_event,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mvien_standard_fields_one_hot() {
        let ssoft = Mvien::from_bits(1 << 1);
        assert!(ssoft.ssoft());
        assert!(!ssoft.sext());
        assert!(!ssoft.counter_overflow());

        let sext = Mvien::from_bits(1 << 9);
        assert!(!sext.ssoft());
        assert!(sext.sext());
        assert!(!sext.counter_overflow());

        let counter_overflow = Mvien::from_bits(1 << 13);
        assert!(!counter_overflow.ssoft());
        assert!(!counter_overflow.sext());
        assert!(counter_overflow.counter_overflow());
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn mvien_ras_fields_one_hot() {
        let low = Mvien::from_bits(1usize << 35);
        assert!(low.low_priority_ras_event());
        assert!(!low.high_priority_ras_event());

        let high = Mvien::from_bits(1usize << 43);
        assert!(!high.low_priority_ras_event());
        assert!(high.high_priority_ras_event());
    }

    #[test]
    fn mvien_mask() {
        let expected = usize::MAX & !0x1DFD;
        assert_eq!(Mvien::BITMASK, expected);
        assert_eq!(Mvien::from_bits(usize::MAX).bits(), expected);
        assert_eq!(Mvien::from_bits(1 << 5).bits(), 0);
    }
}
