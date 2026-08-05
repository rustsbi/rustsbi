//! Virtual supervisor indirect register alias (vsireg)

riscv::read_write_csr! {
    /// Virtual supervisor indirect register alias.
    Vsireg: 0x251,
    mask: usize::MAX,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vsireg_raw_roundtrip() {
        let bits: usize = 0xABCD_EF01usize;
        let reg = Vsireg::from_bits(bits);
        assert_eq!(reg.bits(), bits);
    }
}
