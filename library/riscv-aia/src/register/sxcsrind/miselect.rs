//! Machine indirect register select (miselect)

riscv::read_write_csr! {
    /// Machine indirect register select.
    Miselect: 0x350,
    mask: usize::MAX,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn miselect_set_get() {
        let sel: usize = 0x42;
        let s = Miselect::from_bits(sel);
        assert_eq!(s.bits(), sel);
    }
}
