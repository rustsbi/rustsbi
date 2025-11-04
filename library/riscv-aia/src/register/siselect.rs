//! Supervisor indirect register select (siselect)

riscv::read_write_csr! {
    /// Supervisor indirect register select.
    Siselect: 0x150,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}

impl Siselect {
    /// Current value of `siselect` as usize.
    #[inline]
    pub const fn value(self) -> usize {
        self.bits
    }

    // Note: writing to `siselect` should be done via the generated CSR API.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn siselect_value() {
        let sel: usize = 0x42;
        let reg = Siselect::from_bits(sel);
        assert_eq!(reg.value(), sel);
    }
}
