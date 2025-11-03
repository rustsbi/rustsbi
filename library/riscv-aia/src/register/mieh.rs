//! Machine interrupt-enable high-half (RV32 only) (mieh)

riscv::read_write_csr! {
    /// Upper 32 bits of mie (RV32 only).
    Mieh: 0x314,
    mask: 0xFFFF_FFFF,
}

impl Mieh {
    #[inline]
    pub const fn raw(self) -> usize {
        self.bits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mieh_raw_roundtrip() {
        let bits: usize = 0xDEADBEEFusize & 0xFFFF_FFFF;
        let reg = Mieh::from_bits(bits);
        assert_eq!(reg.raw(), bits);
    }
}
