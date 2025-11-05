//! Hypervisor VIPRIO1 high-half (hviprio1h) (RV32 only)

riscv::read_write_csr! {
    /// Upper 32 bits of hviprio1.
    Hviprio1h: 0x656,
    mask: 0xFFFF_FFFF,
}

impl Hviprio1h {
    #[inline]
    pub const fn raw(self) -> usize {
        self.bits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hviprio1h_raw_roundtrip() {
        let bits: usize = 0xDEAD_BEEFusize & 0xFFFF_FFFF;
        let reg = Hviprio1h::from_bits(bits);
        assert_eq!(reg.raw(), bits);
    }
}
