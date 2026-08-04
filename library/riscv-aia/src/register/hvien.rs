//! Hypervisor virtual interrupt enables (hvien)

// Defined in the specification, Section 6.3, page 71:
//
// > Each bit of registers hvien and hvip corresponds with an interrupt number in the range 0-63. Bits
// > 12:0 of hvien are reserved and must be read-only zeros, while bits 12:0 of hvip are defined by the H
// > extension.

riscv::read_write_csr! {
    /// Hypervisor virtual interrupt enables.
    Hvien: 0x608,
    // 0xFFFF_E000 in RV32, or 0xFFFF_FFFF_FFFF_E000 in RV64
    // bits 12:0 are reserved
    mask: usize::MAX & !0x1FFF,
}

riscv::set!(0x608);
riscv::clear!(0x608);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hvien_mask() {
        let expected = usize::MAX & !0x1FFF;
        assert_eq!(Hvien::BITMASK, expected);
        assert_eq!(Hvien::from_bits(usize::MAX).bits(), expected);
    }
}
