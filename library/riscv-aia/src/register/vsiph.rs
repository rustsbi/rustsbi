//! Virtual supervisor interrupt-pending high-half (vsiph) (RV32 only)

riscv::read_write_csr! {
    /// Upper 32 bits of vsip.
    Vsiph: 0x254,
    mask: 0xFFFF_FFFF,
}

impl Vsiph {
    #[inline]
    pub const fn raw(self) -> usize {
        self.bits
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::register::vsieh::Vsieh;

    #[test]
    fn vsieh_vsiph_raw_roundtrip() {
        let bits: usize = 0x8765_4321usize & 0xFFFF_FFFF;
        let e = Vsieh::from_bits(bits);
        let p = Vsiph::from_bits(bits);
        assert_eq!(e.raw(), bits);
        assert_eq!(p.raw(), bits);
    }
}
