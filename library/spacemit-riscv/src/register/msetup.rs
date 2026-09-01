//! Machine setup register.
//!
//! # Platform support
//!
//! This register is supported on SpacemiT X60, X100, and A100 cores. Public
//! K1 and K3 firmware access it on all three core families.
//!
//! The field meanings are inferred from public K1 and K3 firmware. They are
//! not defined by a public SpacemiT hardware specification.
//!
//! [K1 definition](https://github.com/riscv-software-src/opensbi/blob/1f84ec2ac22eaa866d7750f5d7941eb8711fadfc/platform/generic/include/spacemit/k1.h#L4-L24)
//! [K3 OpenSBI source](https://github.com/spacemit-com/opensbi/blob/8bd2cbdf9856dbc1a990d36e26bf47411f356c42/firmware/fw_base.S#L338-L355)

riscv::read_write_csr! {
    /// Machine setup register.
    Msetup: 0x7c0,
    mask: 0x0001_0073,
}

riscv::read_write_csr_field! {
    Msetup,
    /// Data-cache enable.
    ///
    /// This meaning is inferred from public K1 and K3 firmware.
    de: 0,
}

riscv::read_write_csr_field! {
    Msetup,
    /// Instruction-cache enable.
    ///
    /// This meaning is inferred from public K1 and K3 firmware.
    ie: 1,
}

riscv::read_write_csr_field! {
    Msetup,
    /// Branch-prediction enable.
    ///
    /// This meaning is inferred from public K1 and K3 firmware.
    bpe: 4,
}

riscv::read_write_csr_field! {
    Msetup,
    /// Prefetch enable.
    ///
    /// This meaning is inferred from public K1 and K3 firmware.
    pfe: 5,
}

riscv::read_write_csr_field! {
    Msetup,
    /// Misaligned-memory-access enable.
    ///
    /// This meaning is inferred from K1 firmware. K3 firmware writes the same
    /// complete setup value but does not name this field independently.
    mme: 6,
}

riscv::read_write_csr_field! {
    Msetup,
    /// ECC enable.
    ///
    /// This meaning is inferred from K1 firmware. K3 firmware writes the same
    /// complete setup value but does not name this field independently.
    ecce: 16,
}

riscv::set!(0x7c0);
riscv::clear!(0x7c0);

riscv::set_clear_csr!(
    /// Data-cache enable.
    ///
    /// This meaning is inferred from public firmware.
    , set_de, clear_de, 1 << 0);
riscv::set_clear_csr!(
    /// Instruction-cache enable.
    ///
    /// This meaning is inferred from public firmware.
    , set_ie, clear_ie, 1 << 1);
riscv::set_clear_csr!(
    /// Branch-prediction enable.
    ///
    /// This meaning is inferred from public firmware.
    , set_bpe, clear_bpe, 1 << 4);
riscv::set_clear_csr!(
    /// Prefetch enable.
    ///
    /// This meaning is inferred from public firmware.
    , set_pfe, clear_pfe, 1 << 5);
riscv::set_clear_csr!(
    /// Misaligned-memory-access enable.
    ///
    /// This meaning is inferred from public firmware.
    , set_mme, clear_mme, 1 << 6);
riscv::set_clear_csr!(
    /// ECC enable.
    ///
    /// This meaning is inferred from public firmware.
    , set_ecce, clear_ecce, 1 << 16);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn msetup_fields() {
        let mut value = Msetup::from_bits(0);
        assert_eq!(Msetup::BITMASK, 0x0001_0073);

        value.set_de(true);
        value.set_ie(true);
        value.set_bpe(true);
        value.set_pfe(true);
        value.set_mme(true);
        value.set_ecce(true);

        assert!(value.de());
        assert!(value.ie());
        assert!(value.bpe());
        assert!(value.pfe());
        assert!(value.mme());
        assert!(value.ecce());
        assert_eq!(value.bits(), 0x0001_0073);

        value.set_pfe(false);
        assert!(!value.pfe());
        assert_eq!(value.bits(), 0x0001_0053);
    }
}
