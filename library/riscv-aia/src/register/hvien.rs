//! Hypervisor virtual interrupt enables (hvien)

riscv::read_write_csr! {
    /// Hypervisor virtual interrupt enables.
    Hvien: 0x608,
    // 0xFFFF_E000 in RV32, or 0xFFFF_FFFF_FFFF_E000 in RV64
    // bits 12:0 are reserved
    mask: usize::MAX & !0x1FFF,
}

riscv::read_write_csr_field! {
    Hvien,
    /// Counter overflow interrupt virtual enable.
    counter_overflow: 13,
}

#[cfg(not(target_pointer_width = "32"))]
riscv::read_write_csr_field! {
    Hvien,
    /// Low-priority RAS event interrupt virtual enable.
    low_priority_ras_event: 35,
}

#[cfg(not(target_pointer_width = "32"))]
riscv::read_write_csr_field! {
    Hvien,
    /// High-priority RAS event interrupt virtual enable.
    high_priority_ras_event: 43,
}

riscv::set!(0x608);
riscv::clear!(0x608);

riscv::set_clear_csr!(
    /// Counter overflow interrupt virtual enable.
    , set_counter_overflow, clear_counter_overflow, 1 << 13);

#[cfg(not(target_pointer_width = "32"))]
riscv::set_clear_csr!(
    /// Low-priority RAS event interrupt virtual enable.
    , set_low_priority_ras_event, clear_low_priority_ras_event, 1usize << 35);
#[cfg(not(target_pointer_width = "32"))]
riscv::set_clear_csr!(
    /// High-priority RAS event interrupt virtual enable.
    , set_high_priority_ras_event, clear_high_priority_ras_event, 1usize << 43);

#[cfg(target_pointer_width = "32")]
pub use super::hvienh::{
    clear_high_priority_ras_event, clear_low_priority_ras_event, set_high_priority_ras_event,
    set_low_priority_ras_event,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hvien_mask() {
        let expected = usize::MAX & !0x1FFF;
        assert_eq!(Hvien::BITMASK, expected);
        assert_eq!(Hvien::from_bits(usize::MAX).bits(), expected);
    }

    #[test]
    fn hvien_counter_overflow() {
        assert!(Hvien::from_bits(1 << 13).counter_overflow());
    }

    #[cfg(not(target_pointer_width = "32"))]
    #[test]
    fn hvien_ras_fields_are_one_hot() {
        let low = Hvien::from_bits(1usize << 35);
        assert!(low.low_priority_ras_event());
        assert!(!low.high_priority_ras_event());

        let high = Hvien::from_bits(1usize << 43);
        assert!(!high.low_priority_ras_event());
        assert!(high.high_priority_ras_event());
    }
}
