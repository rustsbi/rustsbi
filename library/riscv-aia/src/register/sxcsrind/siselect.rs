//! Supervisor indirect register select (siselect)

riscv::read_write_csr! {
    /// Supervisor indirect register select.
    Siselect: 0x150,
    mask: usize::MAX,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn siselect_set_get() {
        let sel: usize = 0x42;
        let reg = Siselect::from_bits(sel);
        assert_eq!(reg.bits(), sel);
    }
}
