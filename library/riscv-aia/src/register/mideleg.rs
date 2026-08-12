//! Machine interrupt delegation (mideleg).

riscv::read_write_csr! {
    /// Machine interrupt delegation.
    Mideleg: 0x303,
    // 0xFFFF_F666 in RV32, or 0xFFFF_FFFF_FFFF_F666 in RV64
    mask: usize::MAX & !0x999,
}

riscv::read_write_csr_field! {
    Mideleg,
    /// Supervisor Software Interrupt Delegate
    ssoft: 1,
}

riscv::read_write_csr_field! {
    Mideleg,
    /// Supervisor Timer Interrupt Delegate
    stimer: 5,
}

riscv::read_write_csr_field! {
    Mideleg,
    /// Supervisor External Interrupt Delegate
    sext: 9,
}

riscv::read_only_csr_field! {
    Mideleg,
    /// Virtual Supervisor Software Interrupt delegation (read-only one with H).
    vssoft: 2,
}

riscv::read_only_csr_field! {
    Mideleg,
    /// Virtual Supervisor Timer Interrupt delegation (read-only one with H).
    vstimer: 6,
}

riscv::read_only_csr_field! {
    Mideleg,
    /// Virtual Supervisor External Interrupt delegation (read-only one with H).
    vsext: 10,
}

riscv::read_only_csr_field! {
    Mideleg,
    /// Supervisor guest external interrupt delegation.
    /// Read-only one when H is implemented and GEILEN is nonzero.
    sguest_external: 12,
}

riscv::read_write_csr_field! {
    Mideleg,
    /// Counter overflow interrupt delegation.
    counter_overflow: 13,
}

#[cfg(target_pointer_width = "64")]
riscv::read_write_csr_field! {
    Mideleg,
    /// Low-priority RAS event interrupt delegation.
    low_priority_ras_event: 35,
}

#[cfg(target_pointer_width = "64")]
riscv::read_write_csr_field! {
    Mideleg,
    /// High-priority RAS event interrupt delegation.
    high_priority_ras_event: 43,
}

riscv::set!(0x303);
riscv::clear!(0x303);

riscv::set_clear_csr!(
    /// Supervisor Software Interrupt delegation.
    , set_ssoft, clear_ssoft, 1 << 1);
riscv::set_clear_csr!(
    /// Supervisor Timer Interrupt delegation.
    , set_stimer, clear_stimer, 1 << 5);
riscv::set_clear_csr!(
    /// Supervisor External Interrupt delegation.
    , set_sext, clear_sext, 1 << 9);
riscv::set_clear_csr!(
    /// Counter overflow interrupt delegation.
    , set_counter_overflow, clear_counter_overflow, 1 << 13);

#[cfg(target_pointer_width = "64")]
riscv::set_clear_csr!(
    /// Low-priority RAS event interrupt delegation.
    , set_low_priority_ras_event, clear_low_priority_ras_event, 1 << 35);
#[cfg(target_pointer_width = "64")]
riscv::set_clear_csr!(
    /// High-priority RAS event interrupt delegation.
    , set_high_priority_ras_event, clear_high_priority_ras_event, 1 << 43);

#[cfg(target_pointer_width = "32")]
pub use super::midelegh::{
    clear_high_priority_ras_event, clear_low_priority_ras_event, set_high_priority_ras_event,
    set_low_priority_ras_event,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mideleg_low_fields_parse() {
        let bits = 0x3666;
        let md = Mideleg::from_bits(bits);
        assert!(md.ssoft());
        assert!(md.stimer());
        assert!(md.sext());
        assert!(md.vssoft());
        assert!(md.vstimer());
        assert!(md.vsext());
        assert!(md.sguest_external());
        assert!(md.counter_overflow());
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn mideleg_ras_fields_one_hot() {
        let low = Mideleg::from_bits(1usize << 35);
        assert!(low.low_priority_ras_event());
        assert!(!low.high_priority_ras_event());

        let high = Mideleg::from_bits(1usize << 43);
        assert!(!high.low_priority_ras_event());
        assert!(high.high_priority_ras_event());
    }

    #[test]
    fn mideleg_mask() {
        let expected = usize::MAX & !0x999;
        assert_eq!(Mideleg::BITMASK, expected);
        assert_eq!(Mideleg::from_bits(usize::MAX).bits(), expected);
    }
}
