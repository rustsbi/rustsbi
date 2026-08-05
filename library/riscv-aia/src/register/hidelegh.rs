//! Upper 32 bits of Hypervisor interrupt delegation (hidelegh) (RV32 only)

riscv::read_write_csr! {
    /// Upper 32 bits of Hypervisor interrupt delegation.
    Hidelegh: 0x613,
    mask: 0xFFFF_FFFF,
}

riscv::read_write_csr_field! {
    Hidelegh,
    /// Low-priority RAS event interrupt delegation (interrupt 35).
    low_priority_ras_event: 3,
}

riscv::read_write_csr_field! {
    Hidelegh,
    /// High-priority RAS event interrupt delegation (interrupt 43).
    high_priority_ras_event: 11,
}

riscv::set!(0x613);
riscv::clear!(0x613);

riscv::set_clear_csr!(
    /// Low-priority RAS event interrupt delegation.
    , set_low_priority_ras_event, clear_low_priority_ras_event, 1 << 3);
riscv::set_clear_csr!(
    /// High-priority RAS event interrupt delegation.
    , set_high_priority_ras_event, clear_high_priority_ras_event, 1 << 11);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidelegh_ras_fields_are_one_hot() {
        let low = Hidelegh::from_bits(1 << 3);
        assert!(low.low_priority_ras_event());
        assert!(!low.high_priority_ras_event());

        let high = Hidelegh::from_bits(1 << 11);
        assert!(!high.low_priority_ras_event());
        assert!(high.high_priority_ras_event());
    }

    #[test]
    fn hidelegh_mask() {
        assert_eq!(Hidelegh::BITMASK, 0xFFFF_FFFF);
        assert_eq!(Hidelegh::from_bits(usize::MAX).bits(), 0xFFFF_FFFF);
    }
}
