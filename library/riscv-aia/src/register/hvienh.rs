//! Hypervisor virtual interrupt enables high-half (hvienh) (RV32 only)

riscv::read_write_csr! {
    /// Upper 32 bits of hvien.
    Hvienh: 0x618,
    mask: 0xFFFF_FFFF,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hvienh_raw_roundtrip() {
        let bits: usize = 0xDEAD_BEEFusize & 0xFFFF_FFFF;
        let en = Hvienh::from_bits(bits);
        assert_eq!(en.bits(), bits);
    }
}
