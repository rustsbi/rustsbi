//! Hypervisor interrupt delegation (hideleg)

riscv::read_write_csr! {
    /// Hypervisor interrupt delegation.
    Hideleg: 0x603,
    mask: 0x444,
}

riscv::read_write_csr_field! {
    Hideleg,
    /// Virtual Supervisor Software Interrupt delegation.
    vssoft: 2,
}

riscv::read_write_csr_field! {
    Hideleg,
    /// Virtual Supervisor Timer Interrupt delegation.
    vstimer: 6,
}

riscv::read_write_csr_field! {
    Hideleg,
    /// Virtual Supervisor External Interrupt delegation.
    vsext: 10,
}

riscv::set!(0x603);
riscv::clear!(0x603);

riscv::set_clear_csr!(
    /// Virtual Supervisor Software Interrupt delegation.
    , set_vssoft, clear_vssoft, 1 << 2);
riscv::set_clear_csr!(
    /// Virtual Supervisor Timer Interrupt delegation.
    , set_vstime, clear_vstime, 1 << 6);
riscv::set_clear_csr!(
    /// Virtual Supervisor External Interrupt delegation.
    , set_vsext, clear_vsext, 1 << 10);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hideleg_bits() {
        let bits = (1usize << 2) | (1usize << 6) | (1usize << 10);
        let hd = Hideleg::from_bits(bits);
        assert!(hd.vssoft());
        assert!(hd.vstimer());
        assert!(hd.vsext());
    }
}
