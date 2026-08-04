//! Machine virtual interrupt-pending bits (`mvip`).

riscv::read_write_csr! {
    /// Machine virtual interrupt-pending bits.
    Mvip: 0x309,
    // 0xFFFF_E222 in RV32, or 0xFFFF_FFFF_FFFF_E222 in RV64.
    // Among bits 12:0, only bits 1, 5, and 9 are defined.
    mask: usize::MAX & !0x1DDD,
}

riscv::read_only_csr_field! {
    Mvip,
    /// Supervisor software interrupt pending.
    ///
    /// This bit aliases `mip.SSIP` when `mvien[1]` is zero and is an
    /// independent writable bit when `mvien[1]` is one.
    ssoft: 1,
}

riscv::read_only_csr_field! {
    Mvip,
    /// Supervisor timer interrupt pending.
    ///
    /// This bit aliases `mip.STIP` when that bit is writable and is
    /// read-only zero otherwise.
    stimer: 5,
}

riscv::read_only_csr_field! {
    Mvip,
    /// Supervisor external interrupt pending.
    ///
    /// This bit aliases the software-writable part of `mip.SEIP` when
    /// `mvien[9]` is zero and is independent when `mvien[9]` is one.
    sext: 9,
}

riscv::read_only_csr_field! {
    Mvip,
    /// Counter overflow interrupt pending.
    counter_overflow: 13,
}

#[cfg(not(target_pointer_width = "32"))]
riscv::read_only_csr_field! {
    Mvip,
    /// Low-priority RAS event interrupt pending.
    low_priority_ras_event: 35,
}

#[cfg(not(target_pointer_width = "32"))]
riscv::read_only_csr_field! {
    Mvip,
    /// High-priority RAS event interrupt pending.
    high_priority_ras_event: 43,
}

impl Mvip {
    /// Returns whether virtual interrupt `number` is pending.
    ///
    /// `number` must be less than the current XLEN. On RV32, use `mviph`
    /// for interrupt numbers 32-63.
    #[inline]
    pub const fn bit(self, number: usize) -> bool {
        assert!(number < usize::BITS as usize);
        ((self.bits >> number) & 1) != 0
    }
}

riscv::set!(0x309);
riscv::clear!(0x309);

riscv::set_clear_csr!(
    /// Supervisor software interrupt pending.
    , set_ssoft, clear_ssoft, 1 << 1);
riscv::set_clear_csr!(
    /// Supervisor timer interrupt pending, when writable.
    , set_stimer, clear_stimer, 1 << 5);
riscv::set_clear_csr!(
    /// Supervisor external interrupt pending software bit.
    , set_sext, clear_sext, 1 << 9);
riscv::set_clear_csr!(
    /// Counter overflow interrupt pending.
    , set_counter_overflow, clear_counter_overflow, 1 << 13);

#[cfg(not(target_pointer_width = "32"))]
riscv::set_clear_csr!(
    /// Low-priority RAS event interrupt pending.
    , set_low_priority_ras_event, clear_low_priority_ras_event, 1usize << 35);
#[cfg(not(target_pointer_width = "32"))]
riscv::set_clear_csr!(
    /// High-priority RAS event interrupt pending.
    , set_high_priority_ras_event, clear_high_priority_ras_event, 1usize << 43);

#[cfg(target_pointer_width = "32")]
pub use super::mviph::{
    clear_high_priority_ras_event, clear_low_priority_ras_event, set_high_priority_ras_event,
    set_low_priority_ras_event,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mvip_standard_fields_are_one_hot() {
        let ssoft = Mvip::from_bits(1 << 1);
        assert!(ssoft.ssoft());
        assert!(!ssoft.stimer());
        assert!(!ssoft.sext());
        assert!(!ssoft.counter_overflow());

        let stimer = Mvip::from_bits(1 << 5);
        assert!(!stimer.ssoft());
        assert!(stimer.stimer());
        assert!(!stimer.sext());
        assert!(!stimer.counter_overflow());

        let sext = Mvip::from_bits(1 << 9);
        assert!(!sext.ssoft());
        assert!(!sext.stimer());
        assert!(sext.sext());
        assert!(!sext.counter_overflow());

        let counter_overflow = Mvip::from_bits(1 << 13);
        assert!(!counter_overflow.ssoft());
        assert!(!counter_overflow.stimer());
        assert!(!counter_overflow.sext());
        assert!(counter_overflow.counter_overflow());
    }

    #[cfg(not(target_pointer_width = "32"))]
    #[test]
    fn mvip_ras_fields_are_one_hot() {
        let low = Mvip::from_bits(1usize << 35);
        assert!(low.low_priority_ras_event());
        assert!(!low.high_priority_ras_event());

        let high = Mvip::from_bits(1usize << 43);
        assert!(!high.low_priority_ras_event());
        assert!(high.high_priority_ras_event());
    }

    #[test]
    fn mvip_mask() {
        let expected = usize::MAX & !0x1DDD;
        assert_eq!(Mvip::BITMASK, expected);
        assert_eq!(Mvip::from_bits(usize::MAX).bits(), expected);
    }
}
