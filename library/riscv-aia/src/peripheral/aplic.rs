//! Advanced Platform-level Interrupt Controller (APLIC) peripheral.

use volatile_register::{RW, WO};

/// Advanced Platform-level Interrupt Controller (APLIC) register block.
#[repr(C)]
pub struct Aplic {
    /// 0x0000 - Domain configuration.
    pub domaincfg: RW<DomainConfig>,
    /// 0x0004 ..= 0x0FFC - Source configurations.
    pub sourcecfg: [RW<u32>; 1023],
    _padding_0x1000: [u8; 0xBC0],
    /// 0x1BC0 - Machine MSI address configuration, low half.
    pub mmsiaddrcfg: RW<u32>,
    /// 0x1BC4 - Machine MSI address configuration, high half.
    pub mmsiaddrcfgh: RW<u32>,
    /// 0x1BC8 - Supervisor MSI address configuration, low half.
    pub smsiaddrcfg: RW<u32>,
    /// 0x1BCC - Supervisor MSI address configuration, high half.
    pub smsiaddrcfgh: RW<u32>,
    _padding_0x1bd0: [u8; 0x30],
    /// 0x1C00 ..= 0x1C7C - Set interrupt-pending bits (`setip[0..31]`).
    pub setip: [RW<u32>; 32],
    _padding_0x1c80: [u8; 0x5C],
    /// 0x1CDC - Set interrupt-pending bit by number.
    pub setipnum: WO<u32>,
    _padding_0x1ce0: [u8; 0x20],
    /// 0x1D00 ..= 0x1D7C - Rectified inputs & clear pending bits (`in_clrip[0..31]`).
    pub in_clrip: [RW<u32>; 32],
    _padding_0x1d80: [u8; 0x5C],
    /// 0x1DDC - Clear interrupt-pending bit by number.
    pub clripnum: WO<u32>,
    _padding_0x1de0: [u8; 0x20],
    /// 0x1E00 ..= 0x1E7C - Set interrupt-enable bits (`setie[0..31]`).
    pub setie: [RW<u32>; 32],
    _padding_0x1e80: [u8; 0x5C],
    /// 0x1EDC - Set interrupt-enable bit by number.
    pub setienum: WO<u32>,
    _padding_0x1ee0: [u8; 0x20],
    /// 0x1F00 ..= 0x1F7C - Clear interrupt-enable bits (`clrie[0..31]`).
    pub clrie: [RW<u32>; 32],
    _padding_0x1f80: [u8; 0x5C],
    /// 0x1FDC - Clear interrupt-enable bit by number.
    pub clrienum: WO<u32>,
    _padding_0x1fe0: [u8; 0x20],
    /// 0x2000 - Set interrupt-pending bit by number, little-endian.
    pub setipnum_le: WO<u32>,
    /// 0x2004 - Set interrupt-pending bit by number, big-endian.
    pub setipnum_be: WO<u32>,
    _padding_0x2008: [u8; 0x0FF8],
    /// 0x3000 - Generate MSI.
    pub genmsi: RW<u32>,
    /// 0x3004 ..= 0x3FFC - Interrupt targets (`target[1..=1023]`)
    pub target: [RW<u32>; 1023],
}

/// Domain configuration register (`domaincfg`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[doc(alias = "domaincfg")]
#[repr(transparent)]
pub struct DomainConfig(u32);

// TODO other structures

#[cfg(test)]
mod tests {
    use super::Aplic;
    use memoffset::{offset_of, span_of};

    #[test]
    fn struct_aplic_offset() {
        assert_eq!(size_of::<Aplic>(), 0x4000);

        assert_eq!(offset_of!(Aplic, domaincfg), 0x0);
        assert_eq!(span_of!(Aplic, sourcecfg), 0x4..0x1000);
        assert_eq!(offset_of!(Aplic, mmsiaddrcfg), 0x1BC0);
        assert_eq!(offset_of!(Aplic, mmsiaddrcfgh), 0x1BC4);
        assert_eq!(offset_of!(Aplic, smsiaddrcfg), 0x1BC8);
        assert_eq!(offset_of!(Aplic, smsiaddrcfgh), 0x1BCC);
        assert_eq!(span_of!(Aplic, setip), 0x1C00..0x1C80);
        assert_eq!(offset_of!(Aplic, setipnum), 0x1CDC);
        assert_eq!(span_of!(Aplic, in_clrip), 0x1D00..0x1D80);
        assert_eq!(offset_of!(Aplic, clripnum), 0x1DDC);
        assert_eq!(span_of!(Aplic, setie), 0x1E00..0x1E80);
        assert_eq!(offset_of!(Aplic, setienum), 0x1EDC);
        assert_eq!(span_of!(Aplic, clrie), 0x1F00..0x1F80);
        assert_eq!(offset_of!(Aplic, clrienum), 0x1FDC);
        assert_eq!(offset_of!(Aplic, setipnum_le), 0x2000);
        assert_eq!(offset_of!(Aplic, setipnum_be), 0x2004);
        assert_eq!(offset_of!(Aplic, genmsi), 0x3000);
        assert_eq!(span_of!(Aplic, target), 0x3004..0x4000);
    }

    // TODO unit tests for functions of DomainConfig and other structures.
}
