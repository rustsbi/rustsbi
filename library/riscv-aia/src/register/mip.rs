//! Machine interrupt-pending bits (mip)

riscv::read_write_csr! {
    /// Machine interrupt-pending bits.
    Mip: 0x344,
    // 0xFFFF_FEEE in RV32, or 0xFFFF_FFFF_FFFF_FEEE in RV64
    mask: usize::MAX & !0x111,
}

riscv::read_only_csr_field! {
    Mip,
    /// Supervisor software interrupt pending.
    ssoft: 1,
}

riscv::read_only_csr_field! {
    Mip,
    /// Virtual supervisor software interrupt pending.
    vssoft: 2,
}

riscv::read_only_csr_field! {
    Mip,
    /// Machine software interrupt pending.
    msoft: 3,
}

riscv::read_only_csr_field! {
    Mip,
    /// Supervisor timer interrupt pending.
    stimer: 5,
}

riscv::read_only_csr_field! {
    Mip,
    /// Virtual supervisor timer interrupt pending.
    vstimer: 6,
}

riscv::read_only_csr_field! {
    Mip,
    /// Machine timer interrupt pending.
    mtimer: 7,
}

riscv::read_only_csr_field! {
    Mip,
    /// Supervisor external interrupt pending.
    sext: 9,
}

riscv::read_only_csr_field! {
    Mip,
    /// Virtual supervisor external interrupt pending.
    vsext: 10,
}

riscv::read_only_csr_field! {
    Mip,
    /// Machine external interrupt pending.
    mext: 11,
}

riscv::read_only_csr_field! {
    Mip,
    /// Supervisor guest external interrupt pending.
    sguest_external: 12,
}

riscv::read_only_csr_field! {
    Mip,
    /// Counter overflow interrupt pending.
    counter_overflow: 13,
}

#[cfg(target_pointer_width = "64")]
riscv::read_only_csr_field! {
    Mip,
    /// Low-priority RAS event interrupt pending.
    low_priority_ras_event: 35,
}

#[cfg(target_pointer_width = "64")]
riscv::read_only_csr_field! {
    Mip,
    /// High-priority RAS event interrupt pending.
    high_priority_ras_event: 43,
}

impl Mip {
    /// Returns whether interrupt `number` is pending.
    ///
    /// `number` must be less than the current XLEN. On RV32, use `miph`
    /// for interrupt numbers 32-63.
    #[inline]
    pub const fn bit(self, number: usize) -> bool {
        assert!(number < usize::BITS as usize);
        ((self.bits >> number) & 1) != 0
    }
}

riscv::set!(0x344);
riscv::clear!(0x344);

riscv::set_clear_csr!(
    /// Supervisor software interrupt pending.
    , set_ssoft, clear_ssoft, 1 << 1);
riscv::set_clear_csr!(
    /// Virtual supervisor software interrupt pending.
    , set_vssoft, clear_vssoft, 1 << 2);
riscv::set_clear_csr!(
    /// Supervisor timer interrupt pending, when writable.
    , set_stimer, clear_stimer, 1 << 5);
riscv::set_clear_csr!(
    /// Supervisor external interrupt pending software bit.
    , set_sext, clear_sext, 1 << 9);
riscv::set_clear_csr!(
    /// Counter overflow interrupt pending.
    , set_counter_overflow, clear_counter_overflow, 1 << 13);

#[cfg(target_pointer_width = "64")]
riscv::set_clear_csr!(
    /// Low-priority RAS event interrupt pending.
    , set_low_priority_ras_event, clear_low_priority_ras_event, 1usize << 35);
#[cfg(target_pointer_width = "64")]
riscv::set_clear_csr!(
    /// High-priority RAS event interrupt pending.
    , set_high_priority_ras_event, clear_high_priority_ras_event, 1usize << 43);

#[cfg(target_pointer_width = "32")]
pub use super::miph::{
    clear_high_priority_ras_event, clear_low_priority_ras_event, set_high_priority_ras_event,
    set_low_priority_ras_event,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mip_standard_fields_parse() {
        assert!(Mip::from_bits(1 << 1).ssoft());
        assert!(Mip::from_bits(1 << 2).vssoft());
        assert!(Mip::from_bits(1 << 3).msoft());
        assert!(Mip::from_bits(1 << 5).stimer());
        assert!(Mip::from_bits(1 << 6).vstimer());
        assert!(Mip::from_bits(1 << 7).mtimer());
        assert!(Mip::from_bits(1 << 9).sext());
        assert!(Mip::from_bits(1 << 10).vsext());
        assert!(Mip::from_bits(1 << 11).mext());
        assert!(Mip::from_bits(1 << 12).sguest_external());
        assert!(Mip::from_bits(1 << 13).counter_overflow());
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn mip_ras_fields_one_hot() {
        let low = Mip::from_bits(1usize << 35);
        assert!(low.low_priority_ras_event());
        assert!(!low.high_priority_ras_event());

        let high = Mip::from_bits(1usize << 43);
        assert!(!high.low_priority_ras_event());
        assert!(high.high_priority_ras_event());
    }

    #[test]
    fn mip_mask() {
        let expected = usize::MAX & !0x111;
        assert_eq!(Mip::BITMASK, expected);
        assert_eq!(Mip::from_bits(usize::MAX).bits(), expected);
    }
}
