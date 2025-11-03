//! Hypervisor interrupt delegation high-half (hidelegh) (RV32 only)

riscv::read_write_csr! {
    /// Upper 32 bits of hideleg.
    Hidelegh: 0x613,
    mask: 0xFFFF_FFFF,
}

impl Hidelegh {
    /// Raw 32-bit value of `hidelegh`.
    #[inline]
    pub const fn raw(self) -> usize {
        self.bits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidelegh_raw_roundtrip() {
        let bits: usize = 0xDEAD_BEEFusize & 0xFFFF_FFFF;
        let reg = Hidelegh::from_bits(bits);
        assert_eq!(reg.raw(), bits);
    }
}
