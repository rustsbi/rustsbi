//! Supervisor interrupt-enable high-half (sieh) (RV32 only)

riscv::read_write_csr! {
    /// Upper 32 bits of sie (RV32 only).
    Sieh: 0x114,
    mask: 0xFFFF_FFFF,
}

impl Sieh {
    #[inline]
    pub const fn raw(self) -> usize {
        self.bits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sieh_raw_roundtrip() {
        let bits: usize = 0xA5A5A5A5usize & 0xFFFF_FFFF;
        let reg = Sieh::from_bits(bits);
        assert_eq!(reg.raw(), bits);
    }
}
