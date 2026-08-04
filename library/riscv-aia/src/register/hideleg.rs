//! Hypervisor interrupt delegation (hideleg)

// Defined in the specification, Section 5.3, page 62:
//
// > When the H extension is implemented, if a bit is zero in the same position in both mideleg and mvien,
// > then that bit is read-only zero in hideleg (in addition to being read-only zero in sip, sie, hip, and
// > hie). But if a bit for one of interrupts 13-63 is a one in either mideleg or mvien, then the same bit in
// > hideleg may be writable or may be read-only zero, depending on the implementation.
//
// Bits 13-63 of `hideleg` is writable in some of the implementations.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hideleg_bits() {
        let bits = (1usize << 2) | (1usize << 6) | (1usize << 10);
        let hd = Hideleg::from_bits(bits);
        assert!(hd.vssoft());
        assert!(hd.vstimer());
        assert!(hd.vsext());
    }

    #[test]
    fn hideleg_mask() {
        let expected = usize::MAX & !0x1BBB;
        assert_eq!(Hideleg::BITMASK, expected);
        assert_eq!(Hideleg::from_bits(usize::MAX).bits(), expected);
    }
}
