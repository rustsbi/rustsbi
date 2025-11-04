//! Machine indirect register alias (mireg)

riscv::read_write_csr! {
    /// Machine indirect register alias.
    Mireg: 0x351,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}

impl Mireg {
    /// Raw bits read from `mireg` (convenience accessors - width depends on XLEN).
    #[inline]
    pub const fn raw(self) -> usize {
        self.bits
    }

    /// Raw bits as usize convenience accessor.
    #[inline]
    pub const fn as_usize(self) -> usize {
        self.bits
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::register::miselect::Miselect;

    #[test]
    fn miselect_value_and_mireg_raw() {
        let sel: usize = 0x42;
        let s = Miselect::from_bits(sel);
        assert_eq!(s.value(), sel);

        let rbits: usize = 0x1234_5678usize & 0xFFFF_FFFF_FFFF_FFFF;
        let r = Mireg::from_bits(rbits);
        assert_eq!(r.raw(), rbits);
        assert_eq!(r.as_usize(), rbits);
    }
}
