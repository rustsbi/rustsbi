//! Machine interrupt delegation (mideleg).

#[cfg(not(target_arch = "riscv32"))]
riscv::read_write_csr! {
    /// `mideleg` register
    Mideleg: 0x303,
    mask: 0x0000_0808_0000_0666,
}

#[cfg(target_arch = "riscv32")]
riscv::read_write_csr! {
    /// `mideleg` register
    Mideleg: 0x303,
    mask: 0x0000_0666,
}

riscv::read_write_csr_field! {
    Mideleg,
    /// Supervisor Software Interrupt Delegate
    ssoft: 1,
}

riscv::read_write_csr_field! {
    Mideleg,
    /// Supervisor Timer Interrupt Delegate
    stimer: 5,
}

riscv::read_write_csr_field! {
    Mideleg,
    /// Supervisor External Interrupt Delegate
    sext: 9,
}

riscv::read_write_csr_field! {
    Mideleg,
    /// Virtual Supervisor Software Interrupt delegation.
    vssoft: 2,
}

riscv::read_write_csr_field! {
    Mideleg,
    /// Virtual Supervisor Timer Interrupt delegation.
    vstimer: 6,
}

riscv::read_write_csr_field! {
    Mideleg,
    /// Virtual Supervisor External Interrupt delegation.
    vsext: 10,
}

#[cfg(not(target_arch = "riscv32"))]
riscv::read_write_csr_field! {
    Mideleg,
    /// Low priority RAS event interrupt delegation.
    low_priority_ras_event: 35,
}

#[cfg(not(target_arch = "riscv32"))]
riscv::read_write_csr_field! {
    Mideleg,
    /// High priority RAS event interrupt delegation.
    high_priority_ras_event: 43,
}

riscv::set!(0x303);
riscv::clear!(0x303);

riscv::set_clear_csr!(
    /// Supervisor Software Interrupt delegation.
    , set_ssoft, clear_ssoft, 1 << 1);
riscv::set_clear_csr!(
    /// Supervisor Timer Interrupt delegation.
    , set_stimer, clear_stimer, 1 << 5);
riscv::set_clear_csr!(
    /// Supervisor External Interrupt delegation.
    , set_sext, clear_sext, 1 << 9);
riscv::set_clear_csr!(
    /// Virtual Supervisor Software Interrupt delegation.
    , set_vssoft, clear_vssoft, 1 << 2);
riscv::set_clear_csr!(
    /// Low priority RAS event interrupt delegation.
    , set_vstime, clear_vstime, 1 << 6);
riscv::set_clear_csr!(
    /// Virtual Supervisor External Interrupt delegation.
    , set_vsext, clear_vsext, 1 << 10);

#[cfg(not(target_arch = "riscv32"))]
riscv::set_clear_csr!(
    /// Virtual Supervisor Timer Interrupt delegation.
    , set_low_priority_ras_event, clear_low_priority_ras_event, 1 << 35);
#[cfg(not(target_arch = "riscv32"))]
riscv::set_clear_csr!(
    /// High priority RAS event interrupt delegation.
    , set_high_priority_ras_event, clear_high_priority_ras_event, 1 << 43);

#[cfg(target_arch = "riscv32")]
pub use super::midelegh::{
    clear_high_priority_ras_event, clear_low_priority_ras_event, set_high_priority_ras_event,
    set_low_priority_ras_event,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mideleg_bits() {
        let bits = 0x666;
        let md = Mideleg::from_bits(bits);
        assert!(md.ssoft());
        assert!(md.stimer());
        assert!(md.sext());
        assert!(md.vssoft());
        assert!(md.vstimer());
        assert!(md.vsext());
    }
}
