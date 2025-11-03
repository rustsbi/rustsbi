//! Hypervisor VIPRIO2 high-half (hviprio2h) (RV32 only)

riscv::read_write_csr! {
    /// Upper 32 bits of hviprio2.
    Hviprio2h: 0x657,
    mask: 0xFFFF_FFFF,
}

impl Hviprio2h {
    #[inline]
    pub const fn raw(self) -> usize {
        self.bits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hviprio2h_raw_roundtrip() {
        let bits: usize = 0x1234_5678usize & 0xFFFF_FFFF;
        let reg = Hviprio2h::from_bits(bits);
        assert_eq!(reg.raw(), bits);
    }
}
