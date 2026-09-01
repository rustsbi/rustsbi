//! Physical memory attribute configuration 0 register.
//!
//! # Platform support
//!
//! Public K3 firmware declares this register for SpacemiT X100 and A100 cores
//! and programs it from shared K3 startup assembly. The startup access does not
//! establish a separate field layout for each core family.
//!
//! Public K3 firmware directly programs only the two byte fields exposed here.
//! Their positions and observed values are known from firmware, while their
//! internal bit definitions and the remaining register layout are unknown.
//!
//! [K3 OpenSBI source](https://github.com/spacemit-com/opensbi/blob/8bd2cbdf9856dbc1a990d36e26bf47411f356c42/firmware/fw_base.S#L318-L333)

/// Attribute value for XIP I/O.
///
/// This use is observed in public K3 firmware; the value's internal bit
/// definitions are not publicly specified.
pub const XIP_IO_ATTRIBUTE: usize = 0x22;

/// Attribute value for a cacheable audio buffer.
///
/// This use is observed in public K3 firmware; the value's internal bit
/// definitions are not publicly specified.
pub const AUDIO_BUFFER_CACHEABLE_ATTRIBUTE: usize = 0x20;

riscv::read_write_csr! {
    /// K3 physical memory attribute configuration 0 register.
    Pmacfg0: 0x7de,
    mask: 0x00ff_0000_00ff_0000,
}

riscv::read_write_csr_field! {
    Pmacfg0,
    /// Positional byte field 2.
    ///
    /// Public K3 firmware writes the XIP I/O attribute value to this field.
    attribute2: [16:23],
}

riscv::read_write_csr_field! {
    Pmacfg0,
    /// Positional byte field 6.
    ///
    /// Public K3 firmware writes the cacheable audio-buffer value to this field.
    attribute6: [48:55],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pmacfg0_attribute2() {
        let mut value = Pmacfg0::from_bits(0);
        value.set_attribute2(XIP_IO_ATTRIBUTE);
        assert_eq!(value.attribute2(), XIP_IO_ATTRIBUTE);
        assert_eq!(value.bits(), XIP_IO_ATTRIBUTE << 16);
    }

    #[test]
    fn pmacfg0_attribute6() {
        let mut value = Pmacfg0::from_bits(XIP_IO_ATTRIBUTE << 16);
        value.set_attribute6(AUDIO_BUFFER_CACHEABLE_ATTRIBUTE);
        assert_eq!(value.attribute2(), XIP_IO_ATTRIBUTE);
        assert_eq!(value.attribute6(), AUDIO_BUFFER_CACHEABLE_ATTRIBUTE);
        assert_eq!(
            value.bits(),
            (XIP_IO_ATTRIBUTE << 16) | (AUDIO_BUFFER_CACHEABLE_ATTRIBUTE << 48)
        );
        assert_eq!(Pmacfg0::BITMASK, 0x00ff_0000_00ff_0000);
    }
}
