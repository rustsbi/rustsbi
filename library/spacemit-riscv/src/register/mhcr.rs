//! Machine hardware control register.
//!
//! # Platform support
//!
//! Public firmware declares this register for SpacemiT X60, X100, and A100
//! cores. Current K3 OpenSBI writes bit 0 from a path shared by X100 and A100;
//! the change that introduced the write describes only X100 AI extensions.
//!
//! K1 and K3 firmware identify the register at the same address. The bit 0
//! meaning exposed here is inferred from a K3 firmware change describing X100
//! AI-extension enablement; its meaning on K1 and A100 is unknown.
//!
//! [K1 declaration](https://github.com/riscv-software-src/opensbi/blob/1f84ec2ac22eaa866d7750f5d7941eb8711fadfc/platform/generic/include/spacemit/k1.h#L4-L24)
//! [K3 access](https://github.com/spacemit-com/opensbi/blob/8bd2cbdf9856dbc1a990d36e26bf47411f356c42/lib/sbi/sbi_hart.c#L1093-L1094)
//! [X100 introduction](https://github.com/spacemit-com/opensbi/commit/5b3376282d6f27f555a1167b941c4a8568af61b5)

riscv::read_write_csr! {
    /// Machine hardware control register.
    Mhcr: 0x7c1,
    mask: 0x1,
}

riscv::read_write_csr_field! {
    Mhcr,
    /// X100 AI-extension enable.
    ///
    /// This meaning is inferred from a public K3 firmware change.
    x100_ai_extensions_enable: 0,
}

riscv::set!(0x7c1);
riscv::clear!(0x7c1);

riscv::set_clear_csr!(
    /// X100 AI-extension enable.
    ///
    /// This meaning is inferred from public K3 firmware.
    , set_x100_ai_extensions_enable, clear_x100_ai_extensions_enable, 1 << 0);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mhcr_field() {
        let mut value = Mhcr::from_bits(0);
        assert_eq!(Mhcr::BITMASK, 0x1);
        assert!(!value.x100_ai_extensions_enable());

        value.set_x100_ai_extensions_enable(true);
        assert!(value.x100_ai_extensions_enable());
        assert_eq!(value.bits(), 0x1);
    }
}
