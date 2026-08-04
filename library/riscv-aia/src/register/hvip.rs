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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hvip_bits() {
        // set vssip (bit 2), vstip (bit 6), vseip (bit 10)
        let bits: usize = (1usize << 2) | (1usize << 6) | (1usize << 10);
        let pend = Hvip::from_bits(bits);
        assert!(pend.vssoft());
        assert!(pend.vstimer());
        assert!(pend.vsext());
    }

    #[test]
    fn hvip_mask() {
        let expected = usize::MAX & !0x1BBB;
        assert_eq!(Hvip::BITMASK, expected);
        assert_eq!(Hvip::from_bits(usize::MAX).bits(), expected);
    }
}
