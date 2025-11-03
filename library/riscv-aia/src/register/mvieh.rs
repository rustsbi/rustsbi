//! Machine virtual interrupt enables high-half (mvieh) (RV32 only)

riscv::read_write_csr! {
    /// Upper 32 bits of mvien (RV32 only).
    Mvieh: 0x318,
    mask: 0xFFFF_FFFF,
}

impl Mvieh {
    #[inline]
    pub const fn raw(self) -> usize {
        self.bits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mvieh_raw_roundtrip() {
        let bits: usize = 0xCAFEBABEu32 as usize;
        let reg = Mvieh::from_bits(bits);
        assert_eq!(reg.raw(), bits);
    }
}
