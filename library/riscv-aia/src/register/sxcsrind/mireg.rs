//! Machine indirect register alias (mireg)

riscv::read_write_csr! {
    /// Machine indirect register alias.
    Mireg: 0x351,
    mask: usize::MAX,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mireg_set_get() {
        let rbits: usize = 0x1234_5678usize;
        let r = Mireg::from_bits(rbits);
        assert_eq!(r.bits(), rbits);
    }
}
