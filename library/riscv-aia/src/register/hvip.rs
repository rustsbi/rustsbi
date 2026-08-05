//! Hypervisor virtual interrupt pending bits (hvip)

riscv::read_write_csr! {
    /// Hypervisor virtual interrupt pending bits.
    Hvip: 0x645,
    // 0xFFFF_E444 in RV32, or 0xFFFF_FFFF_FFFF_E444 in RV64
    mask: usize::MAX & !0x1BBB,
}

riscv::read_only_csr_field! {
    Hvip,
    /// Virtual Supervisor Software Interrupt pending.
    vssoft: 2,
}

riscv::read_only_csr_field! {
    Hvip,
    /// Virtual Supervisor Timer Interrupt pending.
    vstimer: 6,
}

riscv::read_only_csr_field! {
    Hvip,
    /// Virtual Supervisor External Interrupt pending.
    vsext: 10,
}

riscv::read_only_csr_field! {
    Hvip,
    /// Counter overflow interrupt pending.
    counter_overflow: 13,
}

#[cfg(not(target_pointer_width = "32"))]
riscv::read_only_csr_field! {
    Hvip,
    /// Low-priority RAS event interrupt pending.
    low_priority_ras_event: 35,
}

#[cfg(not(target_pointer_width = "32"))]
riscv::read_only_csr_field! {
    Hvip,
    /// High-priority RAS event interrupt pending.
    high_priority_ras_event: 43,
}

riscv::set!(0x645);
riscv::clear!(0x645);

riscv::set_clear_csr!(
    /// Virtual Supervisor Software Interrupt pending.
    , set_vssoft, clear_vssoft, 1 << 2);
riscv::set_clear_csr!(
    /// Virtual Supervisor Timer Interrupt pending.
    , set_vstime, clear_vstime, 1 << 6);
riscv::set_clear_csr!(
    /// Virtual Supervisor External Interrupt pending.
    , set_vsext, clear_vsext, 1 << 10);
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
pub use super::hviph::{
    clear_high_priority_ras_event, clear_low_priority_ras_event, set_high_priority_ras_event,
    set_low_priority_ras_event,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hvip_fields_are_one_hot() {
        let vssoft = Hvip::from_bits(1 << 2);
        assert!(vssoft.vssoft());
        assert!(!vssoft.vstimer());
        assert!(!vssoft.vsext());
        assert!(!vssoft.counter_overflow());

        let vstimer = Hvip::from_bits(1 << 6);
        assert!(!vstimer.vssoft());
        assert!(vstimer.vstimer());
        assert!(!vstimer.vsext());
        assert!(!vstimer.counter_overflow());

        let vsext = Hvip::from_bits(1 << 10);
        assert!(!vsext.vssoft());
        assert!(!vsext.vstimer());
        assert!(vsext.vsext());
        assert!(!vsext.counter_overflow());

        let counter_overflow = Hvip::from_bits(1 << 13);
        assert!(!counter_overflow.vssoft());
        assert!(!counter_overflow.vstimer());
        assert!(!counter_overflow.vsext());
        assert!(counter_overflow.counter_overflow());
    }

    #[cfg(not(target_pointer_width = "32"))]
    #[test]
    fn hvip_ras_fields() {
        let low = Hvip::from_bits(1usize << 35);
        assert!(low.low_priority_ras_event());
        assert!(!low.high_priority_ras_event());

        let high = Hvip::from_bits(1usize << 43);
        assert!(!high.low_priority_ras_event());
        assert!(high.high_priority_ras_event());
    }

    #[test]
    fn hvip_mask() {
        let expected = usize::MAX & !0x1BBB;
        assert_eq!(Hvip::BITMASK, expected);
        assert_eq!(Hvip::from_bits(usize::MAX).bits(), expected);
    }
}
