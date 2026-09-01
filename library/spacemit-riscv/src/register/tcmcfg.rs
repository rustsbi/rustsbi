//! Tightly coupled memory configuration register.
//!
//! # Platform support
//!
//! Public firmware accesses this register on SpacemiT X60, X100, and A100
//! cores. The K3 write occurs in a path shared by X100 and A100, although the
//! change that introduced it specifically describes A100 support; its meaning
//! on X100 has not been independently established.
//!
//! Public K1 firmware writes the complete values zero and one to disable and
//! enable TCM, and K3 firmware writes one. Treating bit 0 as an enable field is
//! therefore inferred rather than taken from a public hardware specification.
//!
//! [K1 OpenSBI source](https://github.com/spacemit-com/opensbi/blob/fc02b891b17b8bdc1273a39f80aa374cd99ba9a2/lib/utils/psci/spacemit/plat/plat_pm.c#L71-L82)
//! [K3 OpenSBI source](https://github.com/spacemit-com/opensbi/blob/8bd2cbdf9856dbc1a990d36e26bf47411f356c42/lib/sbi/sbi_hart.c#L1093-L1094)
//! [A100 introduction](https://github.com/spacemit-com/opensbi/commit/2226b80d16707c8c7a0de5c5dcd4b3e52c1903d4)

riscv::read_write_csr! {
    /// Tightly coupled memory configuration register.
    Tcmcfg: 0x5db,
    mask: 0x1,
}

riscv::read_write_csr_field! {
    Tcmcfg,
    /// TCM enable.
    ///
    /// This field is inferred from complete values written by public firmware.
    enable: 0,
}

riscv::set!(0x5db);
riscv::clear!(0x5db);

riscv::set_clear_csr!(
    /// TCM enable.
    ///
    /// This field is inferred from public firmware.
    , set_enable, clear_enable, 1 << 0);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tcmcfg_field() {
        let mut value = Tcmcfg::from_bits(0);
        assert_eq!(Tcmcfg::BITMASK, 0x1);
        assert!(!value.enable());

        value.set_enable(true);
        assert!(value.enable());
        assert_eq!(value.bits(), 0x1);

        value.set_enable(false);
        assert!(!value.enable());
        assert_eq!(value.bits(), 0);
    }
}
