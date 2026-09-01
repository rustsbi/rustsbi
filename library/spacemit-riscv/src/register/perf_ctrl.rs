//! Performance control register.
//!
//! # Platform support
//!
//! Public K3 firmware declares this register for SpacemiT X100 and A100 cores.
//! It accesses `vec_l1_bypass` only on A100; behavior on X100 has not been
//! independently confirmed.
//!
//! Public K3 firmware accesses this register only on A100 harts. The field
//! meaning is inferred from that firmware and is not publicly specified by
//! SpacemiT.
//!
//! [K3 OpenSBI source](https://github.com/spacemit-com/opensbi/blob/8bd2cbdf9856dbc1a990d36e26bf47411f356c42/platform/generic/spacemit/spacemit_k3.c#L217-L230)

riscv::read_write_csr! {
    /// K3 A100 performance control register.
    PerfCtrl: 0x7d0,
    mask: 0x1_0000_0000,
}

riscv::read_write_csr_field! {
    PerfCtrl,
    /// Vector-load L1 bypass.
    ///
    /// This meaning is inferred from public K3 A100 firmware.
    vec_l1_bypass: 32,
}

riscv::set!(0x7d0);
riscv::clear!(0x7d0);

riscv::set_clear_csr!(
    /// Vector-load L1 bypass.
    ///
    /// This meaning is inferred from public K3 A100 firmware.
    , set_vec_l1_bypass, clear_vec_l1_bypass, 1usize << 32);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perf_ctrl_field() {
        let mut value = PerfCtrl::from_bits(0);
        assert_eq!(PerfCtrl::BITMASK, 0x1_0000_0000);
        assert!(!value.vec_l1_bypass());

        value.set_vec_l1_bypass(true);
        assert!(value.vec_l1_bypass());
        assert_eq!(value.bits(), 0x1_0000_0000);
    }
}
