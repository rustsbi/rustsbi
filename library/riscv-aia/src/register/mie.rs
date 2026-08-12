//! Machine interrupt-enable bits (mie)

riscv::read_write_csr! {
    /// Machine interrupt-enable bits.
    Mie: 0x304,
    // 0xFFFF_FEEE in RV32, or 0xFFFF_FFFF_FFFF_FEEE in RV64
    mask: usize::MAX & !0x111,
}

riscv::read_write_csr_field! {
    Mie,
    /// Supervisor software interrupt enable.
    ssoft: 1,
}

riscv::read_write_csr_field! {
    Mie,
    /// Virtual supervisor software interrupt enable.
    vssoft: 2,
}

riscv::read_write_csr_field! {
    Mie,
    /// Machine software interrupt enable.
    msoft: 3,
}

riscv::read_write_csr_field! {
    Mie,
    /// Supervisor timer interrupt enable.
    stimer: 5,
}

riscv::read_write_csr_field! {
    Mie,
    /// Virtual supervisor timer interrupt enable.
    vstimer: 6,
}

riscv::read_write_csr_field! {
    Mie,
    /// Machine timer interrupt enable.
    mtimer: 7,
}

riscv::read_write_csr_field! {
    Mie,
    /// Supervisor external interrupt enable.
    sext: 9,
}

riscv::read_write_csr_field! {
    Mie,
    /// Virtual supervisor external interrupt enable.
    vsext: 10,
}

riscv::read_write_csr_field! {
    Mie,
    /// Machine external interrupt enable.
    mext: 11,
}

riscv::read_write_csr_field! {
    Mie,
    /// Supervisor guest external interrupt enable.
    sguest_external: 12,
}

riscv::read_write_csr_field! {
    Mie,
    /// Counter overflow interrupt enable.
    counter_overflow: 13,
}

#[cfg(target_pointer_width = "64")]
riscv::read_write_csr_field! {
    Mie,
    /// Low-priority RAS event interrupt enable.
    low_priority_ras_event: 35,
}

#[cfg(target_pointer_width = "64")]
riscv::read_write_csr_field! {
    Mie,
    /// High-priority RAS event interrupt enable.
    high_priority_ras_event: 43,
}

impl Mie {
    /// Returns whether interrupt `number` is enabled.
    ///
    /// `number` must be less than the current XLEN. On RV32, use `mieh`
    /// for interrupt numbers 32-63.
    #[inline]
    pub const fn bit(self, number: usize) -> bool {
        assert!(number < usize::BITS as usize);
        ((self.bits >> number) & 1) != 0
    }
}

riscv::set!(0x304);
riscv::clear!(0x304);

riscv::set_clear_csr!(
    /// Supervisor software interrupt enable.
    , set_ssoft, clear_ssoft, 1 << 1);
riscv::set_clear_csr!(
    /// Virtual supervisor software interrupt enable.
    , set_vssoft, clear_vssoft, 1 << 2);
riscv::set_clear_csr!(
    /// Machine software interrupt enable.
    , set_msoft, clear_msoft, 1 << 3);
riscv::set_clear_csr!(
    /// Supervisor timer interrupt enable.
    , set_stimer, clear_stimer, 1 << 5);
riscv::set_clear_csr!(
    /// Virtual supervisor timer interrupt enable.
    , set_vstimer, clear_vstimer, 1 << 6);
riscv::set_clear_csr!(
    /// Machine timer interrupt enable.
    , set_mtimer, clear_mtimer, 1 << 7);
riscv::set_clear_csr!(
    /// Supervisor external interrupt enable.
    , set_sext, clear_sext, 1 << 9);
riscv::set_clear_csr!(
    /// Virtual supervisor external interrupt enable.
    , set_vsext, clear_vsext, 1 << 10);
riscv::set_clear_csr!(
    /// Machine external interrupt enable.
    , set_mext, clear_mext, 1 << 11);
riscv::set_clear_csr!(
    /// Supervisor guest external interrupt enable.
    , set_sguest_external, clear_sguest_external, 1 << 12);
riscv::set_clear_csr!(
    /// Counter overflow interrupt enable.
    , set_counter_overflow, clear_counter_overflow, 1 << 13);

#[cfg(target_pointer_width = "64")]
riscv::set_clear_csr!(
    /// Low-priority RAS event interrupt enable.
    , set_low_priority_ras_event, clear_low_priority_ras_event, 1usize << 35);
#[cfg(target_pointer_width = "64")]
riscv::set_clear_csr!(
    /// High-priority RAS event interrupt enable.
    , set_high_priority_ras_event, clear_high_priority_ras_event, 1usize << 43);

#[cfg(target_pointer_width = "32")]
pub use super::mieh::{
    clear_high_priority_ras_event, clear_low_priority_ras_event, set_high_priority_ras_event,
    set_low_priority_ras_event,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mie_standard_fields_parse() {
        assert!(Mie::from_bits(1 << 1).ssoft());
        assert!(Mie::from_bits(1 << 2).vssoft());
        assert!(Mie::from_bits(1 << 3).msoft());
        assert!(Mie::from_bits(1 << 5).stimer());
        assert!(Mie::from_bits(1 << 6).vstimer());
        assert!(Mie::from_bits(1 << 7).mtimer());
        assert!(Mie::from_bits(1 << 9).sext());
        assert!(Mie::from_bits(1 << 10).vsext());
        assert!(Mie::from_bits(1 << 11).mext());
        assert!(Mie::from_bits(1 << 12).sguest_external());
        assert!(Mie::from_bits(1 << 13).counter_overflow());
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn mie_ras_fields_one_hot() {
        let low = Mie::from_bits(1usize << 35);
        assert!(low.low_priority_ras_event());
        assert!(!low.high_priority_ras_event());

        let high = Mie::from_bits(1usize << 43);
        assert!(!high.low_priority_ras_event());
        assert!(high.high_priority_ras_event());
    }

    #[test]
    fn mie_mask() {
        let expected = usize::MAX & !0x111;
        assert_eq!(Mie::BITMASK, expected);
        assert_eq!(Mie::from_bits(usize::MAX).bits(), expected);
    }
}
