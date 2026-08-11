//! Supervisor indirect register alias (sireg)

riscv::read_write_csr! {
    /// Supervisor indirect register alias.
    Sireg: 0x151,
    mask: usize::MAX,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sireg_set_get() {
        let bits: usize = 0x1234_5678usize;
        let reg = Sireg::from_bits(bits);
        assert_eq!(reg.bits(), bits);
    }
}
