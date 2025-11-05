//! Hypervisor virtual interrupt enables (hvien)

riscv::read_write_csr! {
    /// Hypervisor virtual interrupt enables.
    Hvien: 0x608,
    mask: 0x444,
}

riscv::read_write_csr_field! {
    Hvien,
    /// Virtual Supervisor Software Interrupt enable.
    vssoft: 2,
}

riscv::read_write_csr_field! {
    Hvien,
    /// Virtual Supervisor Timer Interrupt enable.
    vstimer: 6,
}

riscv::read_write_csr_field! {
    Hvien,
    /// Virtual Supervisor External Interrupt enable.
    vsext: 10,
}

riscv::set!(0x608);
riscv::clear!(0x608);

riscv::set_clear_csr!(
    /// Virtual Supervisor Software Interrupt enable.
    , set_vssoft, clear_vssoft, 1 << 2);
riscv::set_clear_csr!(
    /// Virtual Supervisor Timer Interrupt enable.
    , set_vstime, clear_vstime, 1 << 6);
riscv::set_clear_csr!(
    /// Virtual Supervisor External Interrupt enable.
    , set_vsext, clear_vsext, 1 << 10);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hvien_bits() {
        // set vssip (bit 2), vstip (bit 6), vseip (bit 10)
        let bits: usize = (1usize << 2) | (1usize << 6) | (1usize << 10);
        let en = Hvien::from_bits(bits);
        assert!(en.vssoft());
        assert!(en.vstimer());
        assert!(en.vsext());
    }
}
