//! Machine indirect register select (miselect)

riscv::read_write_csr! {
    /// Machine indirect register select.
    Miselect: 0x350,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}

// Note: miselect controls which register is accessed via `mireg`.

impl Miselect {
    /// Current value of `miselect` as usize (convenience accessor).
    #[inline]
    pub const fn value(self) -> usize {
        self.bits
    }

    // Note: writing to `miselect` should be done via the generated CSR API.
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::register::mireg::Mireg;

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
