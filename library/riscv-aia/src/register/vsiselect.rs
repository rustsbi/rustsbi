//! Virtual supervisor indirect register select (vsiselect)

riscv::read_write_csr! {
    /// Virtual supervisor indirect register select.
    Vsiselect: 0x250,
    mask: usize::MAX,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vsiselect_value() {
        let sel: usize = 0x99;
        let reg = Vsiselect::from_bits(sel);
        assert_eq!(reg.bits(), sel);
    }
}
