//! Virtual supervisor indirect register select (vsiselect)

riscv::read_write_csr! {
    /// Virtual supervisor indirect register select.
    Vsiselect: 0x250,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}

impl Vsiselect {
    /// Current value of `vsiselect` as usize.
    #[inline]
    pub const fn value(self) -> usize {
        self.bits
    }

    // Note: writing to `vsiselect` should be done via the generated CSR API.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vsiselect_value() {
        let sel: usize = 0x99;
        let reg = Vsiselect::from_bits(sel);
        assert_eq!(reg.value(), sel);
    }
}
