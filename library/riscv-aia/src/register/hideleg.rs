//! Hypervisor interrupt delegation (hideleg)

// Interrupts 13-63 may be writable depending on `mideleg`, `mvien`, and the implementation.
riscv::read_write_csr! {
    /// Hypervisor interrupt delegation.
    Hideleg: 0x603,
    // 0xFFFF_E444 in RV32, or 0xFFFF_FFFF_FFFF_E444 in RV64
    mask: usize::MAX & !0x1BBB,
}

riscv::read_write_csr_field! {
    Hideleg,
    /// Virtual Supervisor Software Interrupt delegation.
    vssoft: 2,
}

riscv::read_write_csr_field! {
    Hideleg,
    /// Virtual Supervisor Timer Interrupt delegation.
    vstimer: 6,
}

riscv::read_write_csr_field! {
    Hideleg,
    /// Virtual Supervisor External Interrupt delegation.
    vsext: 10,
}

riscv::read_write_csr_field! {
    Hideleg,
    /// Counter overflow interrupt delegation.
    counter_overflow: 13,
}

#[cfg(not(target_pointer_width = "32"))]
riscv::read_write_csr_field! {
    Hideleg,
    /// Low-priority RAS event interrupt delegation.
    low_priority_ras_event: 35,
}

#[cfg(not(target_pointer_width = "32"))]
riscv::read_write_csr_field! {
    Hideleg,
    /// High-priority RAS event interrupt delegation.
    high_priority_ras_event: 43,
}

riscv::set!(0x603);
riscv::clear!(0x603);

riscv::set_clear_csr!(
    /// Virtual Supervisor Software Interrupt delegation.
    , set_vssoft, clear_vssoft, 1 << 2);
riscv::set_clear_csr!(
    /// Virtual Supervisor Timer Interrupt delegation.
    , set_vstime, clear_vstime, 1 << 6);
riscv::set_clear_csr!(
    /// Virtual Supervisor External Interrupt delegation.
    , set_vsext, clear_vsext, 1 << 10);
riscv::set_clear_csr!(
    /// Counter overflow interrupt delegation.
    , set_counter_overflow, clear_counter_overflow, 1 << 13);

#[cfg(not(target_pointer_width = "32"))]
riscv::set_clear_csr!(
    /// Low-priority RAS event interrupt delegation.
    , set_low_priority_ras_event, clear_low_priority_ras_event, 1usize << 35);
#[cfg(not(target_pointer_width = "32"))]
riscv::set_clear_csr!(
    /// High-priority RAS event interrupt delegation.
    , set_high_priority_ras_event, clear_high_priority_ras_event, 1usize << 43);

#[cfg(target_pointer_width = "32")]
pub use super::hidelegh::{
    clear_high_priority_ras_event, clear_low_priority_ras_event, set_high_priority_ras_event,
    set_low_priority_ras_event,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hideleg_fields_are_one_hot() {
        let vssoft = Hideleg::from_bits(1 << 2);
        assert!(vssoft.vssoft());
        assert!(!vssoft.vstimer());
        assert!(!vssoft.vsext());
        assert!(!vssoft.counter_overflow());

        let vstimer = Hideleg::from_bits(1 << 6);
        assert!(!vstimer.vssoft());
        assert!(vstimer.vstimer());
        assert!(!vstimer.vsext());
        assert!(!vstimer.counter_overflow());

        let vsext = Hideleg::from_bits(1 << 10);
        assert!(!vsext.vssoft());
        assert!(!vsext.vstimer());
        assert!(vsext.vsext());
        assert!(!vsext.counter_overflow());

        let counter_overflow = Hideleg::from_bits(1 << 13);
        assert!(!counter_overflow.vssoft());
        assert!(!counter_overflow.vstimer());
        assert!(!counter_overflow.vsext());
        assert!(counter_overflow.counter_overflow());
    }

    #[cfg(not(target_pointer_width = "32"))]
    #[test]
    fn hideleg_ras_fields_are_one_hot() {
        let low = Hideleg::from_bits(1usize << 35);
        assert!(low.low_priority_ras_event());
        assert!(!low.high_priority_ras_event());

        let high = Hideleg::from_bits(1usize << 43);
        assert!(!high.low_priority_ras_event());
        assert!(high.high_priority_ras_event());
    }

    #[test]
    fn hideleg_mask() {
        let expected = usize::MAX & !0x1BBB;
        assert_eq!(Hideleg::BITMASK, expected);
        assert_eq!(Hideleg::from_bits(usize::MAX).bits(), expected);
    }
}
