//! Advanced Platform-level Interrupt Controller (APLIC) peripheral.

use volatile_register::{RW, WO};

/// Advanced Platform-level Interrupt Controller (APLIC) register block.
#[repr(C)]
pub struct Aplic {
    /// 0x0000 - Domain configuration.
    pub domaincfg: RW<DomainConfig>,
    /// 0x0004 ..= 0x0FFC - Source configurations.
    pub sourcecfg: [RW<SourceConfig>; 1023],
    _padding_0x1000: [u8; 0xBC0],
    /// 0x1BC0 - Machine MSI address configuration, low half.
    pub mmsiaddrcfg: RW<u32>,
    /// 0x1BC4 - Machine MSI address configuration, high half.
    pub mmsiaddrcfgh: RW<MachineMsiAddrCfgH>,
    /// 0x1BC8 - Supervisor MSI address configuration, low half.
    pub smsiaddrcfg: RW<u32>,
    /// 0x1BCC - Supervisor MSI address configuration, high half.
    pub smsiaddrcfgh: RW<SupervisorMsiAddrCfgH>,
    _padding_0x1bd0: [u8; 0x30],
    /// 0x1C00 ..= 0x1C7C - Set interrupt-pending bits (`setip[0..31]`).
    pub setip: [RW<SetIntPending>; 32],
    _padding_0x1c80: [u8; 0x5C],
    /// 0x1CDC - Set interrupt-pending bit by number.
    pub setipnum: WO<u32>,
    _padding_0x1ce0: [u8; 0x20],
    /// 0x1D00 ..= 0x1D7C - Rectified inputs & clear pending bits (`in_clrip[0..31]`).
    pub in_clrip: [RW<ClearIntPending>; 32],
    _padding_0x1d80: [u8; 0x5C],
    /// 0x1DDC - Clear interrupt-pending bit by number.
    pub clripnum: WO<u32>,
    _padding_0x1de0: [u8; 0x20],
    /// 0x1E00 ..= 0x1E7C - Set interrupt-enable bits (`setie[0..31]`).
    pub setie: [RW<SetIntEnable>; 32],
    _padding_0x1e80: [u8; 0x5C],
    /// 0x1EDC - Set interrupt-enable bit by number.
    pub setienum: WO<u32>,
    _padding_0x1ee0: [u8; 0x20],
    /// 0x1F00 ..= 0x1F7C - Clear interrupt-enable bits (`clrie[0..31]`).
    pub clrie: [RW<ClearIntEnable>; 32],
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
    pub genmsi: RW<GenerateMSI>,
    /// 0x3004 ..= 0x3FFC - Interrupt targets (`target[1..=1023]`)
    pub target: [RW<IntTarget>; 1023],
}

/// Domain configuration register (`domaincfg`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[doc(alias = "domaincfg")]
#[repr(transparent)]
pub struct DomainConfig(u32);

impl DomainConfig {
    const READ_ONLY: u32 = 0x1 << 31;
    const IE: u32 = 0x1 << 8;
    const DM: u32 = 0x1 << 2;
    const BE: u32 = 0x1;

    /// Get read-only bit (should be true in right endian).
    #[inline]
    pub const fn read_only(self) -> bool {
        (self.0 & Self::READ_ONLY) != 0
    }

    /// Set interrupt-enable bit.
    #[inline]
    pub const fn set_interrupt_enable(self, enable: bool) -> Self {
        if enable {
            Self(self.0 | Self::IE)
        } else {
            Self(self.0 & !Self::IE)
        }
    }

    /// Get interrupt-enable bit.
    #[inline]
    pub const fn interrupt_enable(self) -> bool {
        (self.0 & Self::IE) != 0
    }

    /// Set delivery mode bit.
    #[inline]
    pub const fn set_delivery_mode(self, mode: u8) -> Self {
        assert!(mode < 2, "Delivery mode out of range: 0..=1");
        Self((self.0 & !Self::DM) | ((mode as u32) << 2))
    }

    /// Set big-endian bit.
    #[inline]
    pub const fn set_big_endian(self, be: bool) -> Self {
        if be {
            Self(self.0 | Self::BE)
        } else {
            Self(self.0 & !Self::BE)
        }
    }

    /// Get big-endian bit.
    #[inline]
    pub const fn big_endian(self) -> bool {
        (self.0 & Self::BE) != 0
    }
}

/// Source configuration register (`sourcecfg`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[doc(alias = "sourcecfg")]
#[repr(transparent)]
pub struct SourceConfig(u32);

impl SourceConfig {
    const D: u32 = 0x1 << 10;
    const CHILD_INDEX: u32 = 0x3FF;
    const SM: u32 = 0x7;

    /// Set delegate bit.
    #[inline]
    pub const fn set_delegate(self, delegate: bool) -> Self {
        if delegate {
            Self(self.0 | Self::D)
        } else {
            Self(self.0 & !Self::D)
        }
    }

    /// Get delegate bit.
    #[inline]
    pub const fn delegate(self) -> bool {
        (self.0 & Self::D) != 0
    }

    /// Set child index.
    #[inline]
    pub const fn set_child_index(self, index: u16) -> Self {
        assert!(index < 1024, "Child index out of range: 0..=1023");
        Self((self.0 & !Self::CHILD_INDEX) | (index as u32))
    }

    /// Get child index.
    #[inline]
    pub const fn child_index(self) -> u16 {
        (self.0 & Self::CHILD_INDEX) as u16
    }

    /// Set source mode bit.
    #[inline]
    pub const fn set_source_mode(self, mode: u8) -> Self {
        assert!(mode < 8, "Source mode out of range: 0..=7");
        Self((self.0 & !Self::SM) | ((mode as u32) & Self::SM))
    }

    /// Get source mode bit.
    #[inline]
    pub const fn source_mode(self) -> u8 {
        (self.0 & Self::SM) as u8
    }
}

/// Machine MSI address configuration, high half register (`mmsiaddrcfgh`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[doc(alias = "mmsiaddrcfgh")]
#[repr(transparent)]
pub struct MachineMsiAddrCfgH(u32);

impl MachineMsiAddrCfgH {
    const L: u32 = 0x1 << 31;
    const HHXS: u32 = 0x1F << 24;
    const LHXS: u32 = 0x7 << 20;
    const HHXW: u32 = 0x7 << 16;
    const LHXW: u32 = 0xF << 12;
    const HIGH_BASE_PPN: u32 = 0xFFF;

    /// Set lock bit.
    #[inline]
    pub const fn set_lock(self, lock: bool) -> Self {
        if lock {
            Self(self.0 | Self::L)
        } else {
            Self(self.0 & !Self::L)
        }
    }

    /// Get lock bit.
    #[inline]
    pub const fn lock(self) -> bool {
        (self.0 & Self::L) != 0
    }

    /// Set high hart index shift.
    #[inline]
    pub const fn set_high_hart_index_shift(self, shift: u8) -> Self {
        assert!(shift < 32, "High hart index shift out of range: 0..=31");
        Self((self.0 & !Self::HHXS) | ((shift as u32) << 24))
    }

    /// Get high hart index shift.
    #[inline]
    pub const fn high_hart_index_shift(self) -> u8 {
        ((self.0 & Self::HHXS) >> 24) as u8
    }

    /// Set low hart index shift.
    #[inline]
    pub const fn set_low_hart_index_shift(self, shift: u8) -> Self {
        assert!(shift < 8, "Low hart index shift out of range: 0..=7");
        Self((self.0 & !Self::LHXS) | ((shift as u32) << 20))
    }

    /// Get low hart index shift.
    #[inline]
    pub const fn low_hart_index_shift(self) -> u8 {
        ((self.0 & Self::LHXS) >> 20) as u8
    }

    /// Set high hart index width.
    #[inline]
    pub const fn set_high_hart_index_width(self, width: u8) -> Self {
        assert!(width < 8, "High hart index width out of range: 0..=7");
        Self((self.0 & !Self::HHXW) | ((width as u32) << 16))
    }

    /// Get high hart index width.
    #[inline]
    pub const fn high_hart_index_width(self) -> u8 {
        ((self.0 & Self::HHXW) >> 16) as u8
    }

    /// Set low hart index width.
    #[inline]
    pub const fn set_low_hart_index_width(self, width: u8) -> Self {
        assert!(width < 16, "Low hart index width out of range: 0..=15");
        Self((self.0 & !Self::LHXW) | ((width as u32) << 12))
    }

    /// Get low hart index width.
    #[inline]
    pub const fn low_hart_index_width(self) -> u8 {
        ((self.0 & Self::LHXW) >> 12) as u8
    }

    /// Set high base PPN.
    #[inline]
    pub const fn set_high_base_ppn(self, ppn: u16) -> Self {
        assert!(ppn < 0xFFF, "High base PPN out of range: 0..=0xFFF");
        Self((self.0 & !Self::HIGH_BASE_PPN) | (ppn as u32))
    }

    /// Get high base PPN.
    #[inline]
    pub const fn high_base_ppn(self) -> u16 {
        (self.0 & Self::HIGH_BASE_PPN) as u16
    }
}

/// Supervisor MSI address configuration, high half register (`smsiaddrcfgh`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[doc(alias = "smsiaddrcfgh")]
#[repr(transparent)]
pub struct SupervisorMsiAddrCfgH(u32);

impl SupervisorMsiAddrCfgH {
    const LHXS: u32 = 0x7 << 20;
    const HIGH_BASE_PPN: u32 = 0xFFF;

    /// Set low hart index shift.
    #[inline]
    pub const fn set_low_hart_index_shift(self, shift: u8) -> Self {
        assert!(shift < 8, "Low hart index shift out of range: 0..=7");
        Self((self.0 & !Self::LHXS) | ((shift as u32) << 20))
    }

    /// Get low hart index shift.
    #[inline]
    pub const fn low_hart_index_shift(self) -> u8 {
        ((self.0 & Self::LHXS) >> 20) as u8
    }

    /// Set high base PPN.
    #[inline]
    pub const fn set_high_base_ppn(self, ppn: u16) -> Self {
        assert!(ppn < 0xFFF, "High base PPN out of range: 0..=0xFFF");
        Self((self.0 & !Self::HIGH_BASE_PPN) | (ppn as u32))
    }

    /// Get high base PPN.
    #[inline]
    pub const fn high_base_ppn(self) -> u16 {
        (self.0 & Self::HIGH_BASE_PPN) as u16
    }
}

/// Set interrupt-pending register (`setip`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[doc(alias = "setip")]
#[repr(transparent)]
pub struct SetIntPending(u32);

impl SetIntPending {
    /// Set interrupt-pending bit.
    #[inline]
    pub const fn set_int_pending(self, bit: u32) -> Self {
        Self(bit)
    }

    /// Get interrupt-pending bits.
    #[inline]
    pub const fn int_pending(self) -> u32 {
        self.0
    }
}

/// Rectified inputs & clear pending bits register (`in_clrip`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[doc(alias = "in_clrip")]
#[repr(transparent)]
pub struct ClearIntPending(u32);

impl ClearIntPending {
    /// Clear interrupt-pending bits.
    #[inline]
    pub const fn clear_int_pending(self, bit: u32) -> Self {
        Self(bit)
    }

    /// Get interrupt-pending bits.
    #[inline]
    pub const fn int_pending(self) -> u32 {
        self.0
    }
}

/// Set interrupt-enable bits register (`setie`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[doc(alias = "setie")]
#[repr(transparent)]
pub struct SetIntEnable(u32);

impl SetIntEnable {
    /// Set interrupt-enable bits.
    #[inline]
    pub const fn set_int_enable(self, bit: u32) -> Self {
        Self(bit)
    }

    /// Get interrupt-enable bits.
    #[inline]
    pub const fn int_enable(self) -> u32 {
        self.0
    }
}

/// Clear interrupt-enable bits register (`clrie`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[doc(alias = "clrie")]
#[repr(transparent)]
pub struct ClearIntEnable(u32);

impl ClearIntEnable {
    /// Clear interrupt-enable bits.
    #[inline]
    pub const fn clear_int_enable(self, bit: u32) -> Self {
        Self(bit)
    }

    /// Get interrupt-enable bits.
    #[inline]
    pub const fn int_enable(self) -> u32 {
        self.0
    }
}

/// Generate MSI register (`genmsi`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[doc(alias = "genmsi")]
#[repr(transparent)]
pub struct GenerateMSI(u32);

impl GenerateMSI {
    const HART_INDEX: u32 = 0x3FF << 18;
    const BUSY: u32 = 0x1 << 12;
    const EIID: u32 = 0x7FF;

    /// Set hart index.
    #[inline]
    pub const fn set_hart_index(self, index: u16) -> Self {
        assert!(index < 1024, "Hart index out of range: 0..=1023");
        Self((self.0 & !Self::HART_INDEX) | ((index as u32) << 18))
    }

    /// Get hart index.
    #[inline]
    pub const fn hart_index(self) -> u16 {
        ((self.0 & Self::HART_INDEX) >> 18) as u16
    }

    /// Get busy bit.
    #[inline]
    pub const fn busy(self) -> bool {
        (self.0 & Self::BUSY) != 0
    }

    /// Set external interrupt identity.
    #[inline]
    pub const fn set_eiid(self, eiid: u16) -> Self {
        assert!(
            eiid < 2048,
            "External interrupt identity out of range: 0..=2047"
        );
        Self((self.0 & !Self::EIID) | (eiid as u32))
    }

    /// Get external interrupt identity.
    #[inline]
    pub const fn eiid(self) -> u16 {
        (self.0 & Self::EIID) as u16
    }
}

/// Interrupt targets register (`target`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[doc(alias = "target")]
#[repr(transparent)]
pub struct IntTarget(u32);

impl IntTarget {
    const HART_INDEX: u32 = 0x3FF << 18;
    const GUEST_INDEX: u32 = 0x3F << 12;
    const EIID: u32 = 0x7FF;
    const IPRIO: u32 = 0xFF;

    /// Set hart index.
    #[inline]
    pub const fn set_hart_index(self, index: u16) -> Self {
        assert!(index < 1024, "Hart index out of range: 0..=1023");
        Self((self.0 & !Self::HART_INDEX) | ((index as u32) << 18))
    }

    /// Get hart index.
    #[inline]
    pub const fn hart_index(self) -> u16 {
        ((self.0 & Self::HART_INDEX) >> 18) as u16
    }

    /// Set guest index.
    #[inline]
    pub const fn set_guest_index(self, index: u8) -> Self {
        assert!(index < 64, "Guest index out of range: 0..=63");
        Self((self.0 & !Self::GUEST_INDEX) | ((index as u32) << 12))
    }

    /// Get guest index.
    #[inline]
    pub const fn guest_index(self) -> u8 {
        ((self.0 & Self::GUEST_INDEX) >> 12) as u8
    }

    /// Set external interrupt identity.
    #[inline]
    pub const fn set_eiid(self, eiid: u16) -> Self {
        assert!(
            eiid < 2048,
            "External interrupt identity out of range: 0..=2047"
        );
        Self((self.0 & !Self::EIID) | (eiid as u32))
    }

    /// Get external interrupt identity.
    #[inline]
    pub const fn eiid(self) -> u16 {
        (self.0 & Self::EIID) as u16
    }

    /// Set interrupt priority.
    #[inline]
    pub const fn set_iprio(self, iprio: u8) -> Self {
        Self((self.0 & !Self::IPRIO) | (iprio as u32))
    }

    /// Get interrupt priority.
    #[inline]
    pub const fn iprio(self) -> u8 {
        (self.0 & Self::IPRIO) as u8
    }
}

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
