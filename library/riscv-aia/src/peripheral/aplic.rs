//! Advanced Platform-level Interrupt Controller (APLIC) peripheral.

use volatile_register::{RO, RW, WO};

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
    pub genmsi: RW<GenerateMsi>,
    /// 0x3004 ..= 0x3FFC - Interrupt targets (`target[1..=1023]`)
    pub target: [RW<IntTarget>; 1023],
}

/// Domain configuration register (`domaincfg`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[doc(alias = "domaincfg")]
#[repr(transparent)]
pub struct DomainConfig(u32);

impl DomainConfig {
    const READ_ONLY_MASK: u32 = 0xFF << 24;
    const READ_ONLY_VALUE: u32 = 0x80 << 24;
    const IE: u32 = 0x1 << 8;
    const DM: u32 = 0x1 << 2;
    const BE: u32 = 0x1;

    /// Get read-only bit (should be true in right endian).
    #[inline]
    pub const fn read_only(self) -> bool {
        (self.0 & Self::READ_ONLY_MASK) == Self::READ_ONLY_VALUE
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
        assert!(ppn <= 0xFFF, "High base PPN out of range: 0..=0xFFF");
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
        assert!(ppn <= 0xFFF, "High base PPN out of range: 0..=0xFFF");
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
pub struct GenerateMsi(u32);

impl GenerateMsi {
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
    /// *NOTE:* According to RISC-V AIA specification, priority value 0 is reserved.
    /// Hardware automatically converts priority 0 to 1 when writing to this field.
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

/// Interrupt delivery control (IDC) structure.
#[repr(C)]
pub struct Idc {
    /// 0x00 - Interrupt delivery enable.
    pub idelivery: RW<InterruptDelivery>,
    /// 0x04 - Interrupt force.
    pub iforce: RW<InterruptForce>,
    /// 0x08 - Interrupt enable threshold.
    pub ithreshold: RW<InterruptThreshold>,
    _padding_0x0c: [u8; 0x0C],
    /// 0x18 - Top interrupt.
    pub topi: RO<TopInterrupt>,
    /// 0x1C - Claim top interrupt.
    pub claimi: RO<ClaimInterrupt>,
}

impl Idc {
    #[inline]
    pub const fn size() -> usize {
        0x20
    }
}

impl Aplic {
    pub const IDC_OFFSET: usize = 0x4000;

    /// Access the Interrupt Delivery Control register block for a hart context.
    ///
    /// # Preconditions
    ///
    /// * The caller's `Aplic` mapping must cover at least `IDC_OFFSET + 0x20 * 1024`
    ///   (`0xC000`) bytes. The IDC array lies *outside* the [`Aplic`] struct itself
    ///   (`size_of::<Aplic>() == 0x4000`), so mapping only `size_of::<Aplic>()`
    ///   bytes and calling this method yields a `&Idc` into unmapped memory.
    /// * The domain must be in direct delivery mode (`domaincfg`.DM = 0): in
    ///   MSI delivery mode (DM = 1) the IDC registers are not used and the IDC
    ///   array may be absent from the address map.
    /// * `hart_index` must be less than the number of implemented hart contexts,
    ///   which may be smaller than the architectural maximum.
    ///
    /// # Panics
    ///
    /// Panics if `hart_index >= 1024` (the architectural maximum).
    #[inline]
    pub fn idc(&self, hart_index: usize) -> &Idc {
        assert!(hart_index < 1024, "Hart index out of range: 0..=1023");
        unsafe {
            &*((self as *const Self as *const u8).add(Self::IDC_OFFSET + hart_index * Idc::size())
                as *const Idc)
        }
    }
}

/// Interrupt delivery enable register (`idelivery`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[doc(alias = "idelivery")]
#[repr(transparent)]
pub struct InterruptDelivery(u32);

impl InterruptDelivery {
    pub const DISABLED: Self = Self(0);

    pub const ENABLED: Self = Self(1);

    #[inline]
    pub const fn set_delivery_enable(self, enable: bool) -> Self {
        if enable {
            Self::ENABLED
        } else {
            Self::DISABLED
        }
    }

    #[inline]
    pub const fn delivery_enable(self) -> bool {
        self.0 == Self::ENABLED.0
    }
}

/// Interrupt force register (`iforce`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[doc(alias = "iforce")]
#[repr(transparent)]
pub struct InterruptForce(u32);

impl InterruptForce {
    pub const NOT_FORCED: Self = Self(0);

    pub const FORCED: Self = Self(1);

    #[inline]
    pub const fn set_force(self, force: bool) -> Self {
        if force {
            Self::FORCED
        } else {
            Self::NOT_FORCED
        }
    }

    #[inline]
    pub const fn force(self) -> bool {
        self.0 == Self::FORCED.0
    }
}

/// Interrupt enable threshold register (`ithreshold`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[doc(alias = "ithreshold")]
#[repr(transparent)]
pub struct InterruptThreshold(u32);

impl InterruptThreshold {
    #[inline]
    pub const fn set_threshold(self, threshold: u8) -> Self {
        Self(threshold as u32)
    }

    #[inline]
    pub const fn threshold(self) -> u8 {
        self.0 as u8
    }
}

/// Top interrupt register (`topi`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[doc(alias = "topi")]
#[repr(transparent)]
pub struct TopInterrupt(u32);

impl TopInterrupt {
    const INTERRUPT_IDENTITY: u32 = 0x3FF << 16;
    const INTERRUPT_PRIORITY: u32 = 0xFF;

    pub const NONE: Self = Self(0);

    #[inline]
    pub const fn interrupt_identity(self) -> u16 {
        ((self.0 & Self::INTERRUPT_IDENTITY) >> 16) as u16
    }

    #[inline]
    pub const fn priority(self) -> u8 {
        (self.0 & Self::INTERRUPT_PRIORITY) as u8
    }

    #[inline]
    pub const fn is_pending(self) -> bool {
        self.0 != 0
    }
}

/// Claim top interrupt register (`claimi`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[doc(alias = "claimi")]
#[repr(transparent)]
pub struct ClaimInterrupt(u32);

impl ClaimInterrupt {
    const INTERRUPT_IDENTITY: u32 = 0x3FF << 16;
    const INTERRUPT_PRIORITY: u32 = 0xFF;

    pub const NONE: Self = Self(0);

    #[inline]
    pub const fn interrupt_identity(self) -> u16 {
        ((self.0 & Self::INTERRUPT_IDENTITY) >> 16) as u16
    }

    #[inline]
    pub const fn priority(self) -> u8 {
        (self.0 & Self::INTERRUPT_PRIORITY) as u8
    }

    #[inline]
    pub const fn is_pending(self) -> bool {
        self.0 != 0
    }
}

#[cfg(test)]
mod tests {
    use super::{Aplic, Idc};
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

    #[test]
    fn struct_idc_offset() {
        assert_eq!(size_of::<Idc>(), 0x20);

        assert_eq!(offset_of!(Idc, idelivery), 0x00);
        assert_eq!(offset_of!(Idc, iforce), 0x04);
        assert_eq!(offset_of!(Idc, ithreshold), 0x08);
        assert_eq!(offset_of!(Idc, topi), 0x18);
        assert_eq!(offset_of!(Idc, claimi), 0x1C);
    }
}

#[cfg(test)]
mod domain_config_tests {
    use super::DomainConfig;

    #[test]
    fn test_domain_config_read_only() {
        let config = DomainConfig(0x8000_0000); // bits 31:24 = 0x80
        assert!(config.read_only());

        let config = DomainConfig(0x0000_0000); // bits 31:24 = 0x00
        assert!(!config.read_only());
    }

    #[test]
    fn test_domain_config_interrupt_enable() {
        let config = DomainConfig(0x0000_0000);
        assert!(!config.interrupt_enable());

        let config = config.set_interrupt_enable(true);
        assert!(config.interrupt_enable());

        let config = config.set_interrupt_enable(false);
        assert!(!config.interrupt_enable());
    }

    #[test]
    fn test_domain_config_delivery_mode() {
        let config = DomainConfig(0x0000_0000);
        let config = config.set_delivery_mode(0);
        assert_eq!(config.0 & 0x4, 0);

        let config = config.set_delivery_mode(1);
        assert_eq!(config.0 & 0x4, 0x4);
    }

    #[test]
    #[should_panic(expected = "Delivery mode out of range: 0..=1")]
    fn test_domain_config_delivery_mode_out_of_range() {
        let config = DomainConfig(0x0000_0000);
        config.set_delivery_mode(2);
    }

    #[test]
    fn test_domain_config_big_endian() {
        let config = DomainConfig(0x0000_0000);
        assert!(!config.big_endian());

        let config = config.set_big_endian(true);
        assert!(config.big_endian());

        let config = config.set_big_endian(false);
        assert!(!config.big_endian());
    }
}

#[cfg(test)]
mod source_config_tests {
    use super::SourceConfig;

    #[test]
    fn test_source_config_delegate() {
        let config = SourceConfig(0x0000_0000);
        assert!(!config.delegate());

        let config = config.set_delegate(true);
        assert!(config.delegate());

        let config = config.set_delegate(false);
        assert!(!config.delegate());
    }

    #[test]
    fn test_source_config_child_index() {
        let config = SourceConfig(0x0000_0000);
        let config = config.set_child_index(0);
        assert_eq!(config.child_index(), 0);

        let config = config.set_child_index(1023);
        assert_eq!(config.child_index(), 1023);
    }

    #[test]
    #[should_panic(expected = "Child index out of range: 0..=1023")]
    fn test_source_config_child_index_out_of_range() {
        let config = SourceConfig(0x0000_0000);
        config.set_child_index(1024);
    }

    #[test]
    fn test_source_config_source_mode() {
        let config = SourceConfig(0x0000_0000);
        let config = config.set_source_mode(0);
        assert_eq!(config.source_mode(), 0);

        let config = config.set_source_mode(7);
        assert_eq!(config.source_mode(), 7);
    }

    #[test]
    #[should_panic(expected = "Source mode out of range: 0..=7")]
    fn test_source_config_source_mode_out_of_range() {
        let config = SourceConfig(0x0000_0000);
        config.set_source_mode(8);
    }
}

#[cfg(test)]
mod machine_msi_addr_cfg_h_tests {
    use super::MachineMsiAddrCfgH;

    #[test]
    fn test_machine_msi_addr_cfg_h_lock() {
        let config = MachineMsiAddrCfgH(0x0000_0000);
        assert!(!config.lock());

        let config = config.set_lock(true);
        assert!(config.lock());

        let config = config.set_lock(false);
        assert!(!config.lock());
    }

    #[test]
    fn test_machine_msi_addr_cfg_h_high_hart_index_shift() {
        let config = MachineMsiAddrCfgH(0x0000_0000);
        let config = config.set_high_hart_index_shift(0);
        assert_eq!(config.high_hart_index_shift(), 0);

        let config = config.set_high_hart_index_shift(31);
        assert_eq!(config.high_hart_index_shift(), 31);
    }

    #[test]
    #[should_panic(expected = "High hart index shift out of range: 0..=31")]
    fn test_machine_msi_addr_cfg_h_high_hart_index_shift_out_of_range() {
        let config = MachineMsiAddrCfgH(0x0000_0000);
        config.set_high_hart_index_shift(32);
    }

    #[test]
    fn test_machine_msi_addr_cfg_h_low_hart_index_shift() {
        let config = MachineMsiAddrCfgH(0x0000_0000);
        let config = config.set_low_hart_index_shift(0);
        assert_eq!(config.low_hart_index_shift(), 0);

        let config = config.set_low_hart_index_shift(7);
        assert_eq!(config.low_hart_index_shift(), 7);
    }

    #[test]
    #[should_panic(expected = "Low hart index shift out of range: 0..=7")]
    fn test_machine_msi_addr_cfg_h_low_hart_index_shift_out_of_range() {
        let config = MachineMsiAddrCfgH(0x0000_0000);
        config.set_low_hart_index_shift(8);
    }

    #[test]
    fn test_machine_msi_addr_cfg_h_high_hart_index_width() {
        let config = MachineMsiAddrCfgH(0x0000_0000);
        let config = config.set_high_hart_index_width(0);
        assert_eq!(config.high_hart_index_width(), 0);

        let config = config.set_high_hart_index_width(7);
        assert_eq!(config.high_hart_index_width(), 7);
    }

    #[test]
    #[should_panic(expected = "High hart index width out of range: 0..=7")]
    fn test_machine_msi_addr_cfg_h_high_hart_index_width_out_of_range() {
        let config = MachineMsiAddrCfgH(0x0000_0000);
        config.set_high_hart_index_width(8);
    }

    #[test]
    fn test_machine_msi_addr_cfg_h_low_hart_index_width() {
        let config = MachineMsiAddrCfgH(0x0000_0000);
        let config = config.set_low_hart_index_width(0);
        assert_eq!(config.low_hart_index_width(), 0);

        let config = config.set_low_hart_index_width(15);
        assert_eq!(config.low_hart_index_width(), 15);
    }

    #[test]
    #[should_panic(expected = "Low hart index width out of range: 0..=15")]
    fn test_machine_msi_addr_cfg_h_low_hart_index_width_out_of_range() {
        let config = MachineMsiAddrCfgH(0x0000_0000);
        config.set_low_hart_index_width(16);
    }

    #[test]
    fn test_machine_msi_addr_cfg_h_high_base_ppn() {
        let config = MachineMsiAddrCfgH(0x0000_0000);
        let config = config.set_high_base_ppn(0);
        assert_eq!(config.high_base_ppn(), 0);

        let config = config.set_high_base_ppn(0xFFF);
        assert_eq!(config.high_base_ppn(), 0xFFF);
    }

    #[test]
    #[should_panic(expected = "High base PPN out of range: 0..=0xFFF")]
    fn test_machine_msi_addr_cfg_h_high_base_ppn_out_of_range() {
        let config = MachineMsiAddrCfgH(0x0000_0000);
        config.set_high_base_ppn(0x1000);
    }
}

#[cfg(test)]
mod supervisor_msi_addr_cfg_h_tests {
    use super::SupervisorMsiAddrCfgH;

    #[test]
    fn test_supervisor_msi_addr_cfg_h_low_hart_index_shift() {
        let config = SupervisorMsiAddrCfgH(0x0000_0000);
        let config = config.set_low_hart_index_shift(0);
        assert_eq!(config.low_hart_index_shift(), 0);

        let config = config.set_low_hart_index_shift(7);
        assert_eq!(config.low_hart_index_shift(), 7);
    }

    #[test]
    #[should_panic(expected = "Low hart index shift out of range: 0..=7")]
    fn test_supervisor_msi_addr_cfg_h_low_hart_index_shift_out_of_range() {
        let config = SupervisorMsiAddrCfgH(0x0000_0000);
        config.set_low_hart_index_shift(8);
    }

    #[test]
    fn test_supervisor_msi_addr_cfg_h_high_base_ppn() {
        let config = SupervisorMsiAddrCfgH(0x0000_0000);
        let config = config.set_high_base_ppn(0);
        assert_eq!(config.high_base_ppn(), 0);

        let config = config.set_high_base_ppn(0xFFF);
        assert_eq!(config.high_base_ppn(), 0xFFF);
    }

    #[test]
    #[should_panic(expected = "High base PPN out of range: 0..=0xFFF")]
    fn test_supervisor_msi_addr_cfg_h_high_base_ppn_out_of_range() {
        let config = SupervisorMsiAddrCfgH(0x0000_0000);
        config.set_high_base_ppn(0x1000);
    }
}

#[cfg(test)]
mod set_int_pending_tests {
    use super::SetIntPending;

    #[test]
    fn test_set_int_pending() {
        let pending = SetIntPending(0x0000_0000);
        let pending = pending.set_int_pending(0x1234_5678);
        assert_eq!(pending.int_pending(), 0x1234_5678);
    }
}

#[cfg(test)]
mod clear_int_pending_tests {
    use super::ClearIntPending;

    #[test]
    fn test_clear_int_pending() {
        let pending = ClearIntPending(0x0000_0000);
        let pending = pending.clear_int_pending(0x1234_5678);
        assert_eq!(pending.int_pending(), 0x1234_5678);
    }
}

#[cfg(test)]
mod set_int_enable_tests {
    use super::SetIntEnable;

    #[test]
    fn test_set_int_enable() {
        let enable = SetIntEnable(0x0000_0000);
        let enable = enable.set_int_enable(0x1234_5678);
        assert_eq!(enable.int_enable(), 0x1234_5678);
    }
}

#[cfg(test)]
mod clear_int_enable_tests {
    use super::ClearIntEnable;

    #[test]
    fn test_clear_int_enable() {
        let enable = ClearIntEnable(0x0000_0000);
        let enable = enable.clear_int_enable(0x1234_5678);
        assert_eq!(enable.int_enable(), 0x1234_5678);
    }
}

#[cfg(test)]
mod generate_msi_tests {
    use super::GenerateMsi;

    #[test]
    fn test_generate_msi_hart_index() {
        let msi = GenerateMsi(0x0000_0000);
        let msi = msi.set_hart_index(0);
        assert_eq!(msi.hart_index(), 0);

        let msi = msi.set_hart_index(1023);
        assert_eq!(msi.hart_index(), 1023);
    }

    #[test]
    #[should_panic(expected = "Hart index out of range: 0..=1023")]
    fn test_generate_msi_hart_index_out_of_range() {
        let msi = GenerateMsi(0x0000_0000);
        msi.set_hart_index(1024);
    }

    #[test]
    fn test_generate_msi_busy() {
        let msi = GenerateMsi(0x0000_1000); // BUSY bit set
        assert!(msi.busy());

        let msi = GenerateMsi(0x0000_0000); // BUSY bit not set
        assert!(!msi.busy());
    }

    #[test]
    fn test_generate_msi_eiid() {
        let msi = GenerateMsi(0x0000_0000);
        let msi = msi.set_eiid(0);
        assert_eq!(msi.eiid(), 0);

        let msi = msi.set_eiid(2047);
        assert_eq!(msi.eiid(), 2047);
    }

    #[test]
    #[should_panic(expected = "External interrupt identity out of range: 0..=2047")]
    fn test_generate_msi_eiid_out_of_range() {
        let msi = GenerateMsi(0x0000_0000);
        msi.set_eiid(2048);
    }
}

#[cfg(test)]
mod int_target_tests {
    use super::IntTarget;

    #[test]
    fn test_int_target_hart_index() {
        let target = IntTarget(0x0000_0000);
        let target = target.set_hart_index(0);
        assert_eq!(target.hart_index(), 0);

        let target = target.set_hart_index(1023);
        assert_eq!(target.hart_index(), 1023);
    }

    #[test]
    #[should_panic(expected = "Hart index out of range: 0..=1023")]
    fn test_int_target_hart_index_out_of_range() {
        let target = IntTarget(0x0000_0000);
        target.set_hart_index(1024);
    }

    #[test]
    fn test_int_target_guest_index() {
        let target = IntTarget(0x0000_0000);
        let target = target.set_guest_index(0);
        assert_eq!(target.guest_index(), 0);

        let target = target.set_guest_index(63);
        assert_eq!(target.guest_index(), 63);
    }

    #[test]
    #[should_panic(expected = "Guest index out of range: 0..=63")]
    fn test_int_target_guest_index_out_of_range() {
        let target = IntTarget(0x0000_0000);
        target.set_guest_index(64);
    }

    #[test]
    fn test_int_target_eiid() {
        let target = IntTarget(0x0000_0000);
        let target = target.set_eiid(0);
        assert_eq!(target.eiid(), 0);

        let target = target.set_eiid(2047);
        assert_eq!(target.eiid(), 2047);
    }

    #[test]
    #[should_panic(expected = "External interrupt identity out of range: 0..=2047")]
    fn test_int_target_eiid_out_of_range() {
        let target = IntTarget(0x0000_0000);
        target.set_eiid(2048);
    }

    #[test]
    fn test_int_target_iprio() {
        let target = IntTarget(0x0000_0000);
        let target = target.set_iprio(0);
        assert_eq!(target.iprio(), 0);

        let target = target.set_iprio(1);
        assert_eq!(target.iprio(), 1);

        let target = target.set_iprio(255);
        assert_eq!(target.iprio(), 255);
    }
}

#[cfg(test)]
mod interrupt_delivery_tests {
    use super::InterruptDelivery;

    #[test]
    fn test_interrupt_delivery_enable() {
        let delivery = InterruptDelivery::DISABLED;
        assert!(!delivery.delivery_enable());

        let delivery = delivery.set_delivery_enable(true);
        assert!(delivery.delivery_enable());
        assert_eq!(delivery, InterruptDelivery::ENABLED);

        let delivery = delivery.set_delivery_enable(false);
        assert!(!delivery.delivery_enable());
        assert_eq!(delivery, InterruptDelivery::DISABLED);
    }
}

#[cfg(test)]
mod interrupt_force_tests {
    use super::InterruptForce;

    #[test]
    fn test_interrupt_force() {
        let force = InterruptForce::NOT_FORCED;
        assert!(!force.force());

        let force = force.set_force(true);
        assert!(force.force());
        assert_eq!(force, InterruptForce::FORCED);

        let force = force.set_force(false);
        assert!(!force.force());
        assert_eq!(force, InterruptForce::NOT_FORCED);
    }
}

#[cfg(test)]
mod interrupt_threshold_tests {
    use super::InterruptThreshold;

    #[test]
    fn test_interrupt_threshold() {
        let threshold = InterruptThreshold(0x0000_0000);
        assert_eq!(threshold.threshold(), 0);

        let threshold = threshold.set_threshold(0);
        assert_eq!(threshold.threshold(), 0);

        let threshold = threshold.set_threshold(255);
        assert_eq!(threshold.threshold(), 255);
    }
}

#[cfg(test)]
mod top_interrupt_tests {
    use super::TopInterrupt;

    #[test]
    fn test_top_interrupt_fields() {
        // identity = 0x123, priority = 0x45
        let bits: u32 = (0x123 << 16) | 0x45;
        let topi = TopInterrupt(bits);
        assert_eq!(topi.interrupt_identity(), 0x123);
        assert_eq!(topi.priority(), 0x45);
        assert!(topi.is_pending());
    }

    #[test]
    fn test_top_interrupt_none() {
        let topi = TopInterrupt::NONE;
        assert_eq!(topi.interrupt_identity(), 0);
        assert_eq!(topi.priority(), 0);
        assert!(!topi.is_pending());
    }

    #[test]
    fn test_top_interrupt_reserved_bits_ignored() {
        // Bits outside 25:16 and 7:0 are reserved and read as zero.
        let bits: u32 = (0x123 << 16) | 0x45 | 0xF000_0000 | 0x0000_3F00;
        let topi = TopInterrupt(bits);
        assert_eq!(topi.interrupt_identity(), 0x123);
        assert_eq!(topi.priority(), 0x45);
    }
}

#[cfg(test)]
mod claim_interrupt_tests {
    use super::ClaimInterrupt;

    #[test]
    fn test_claim_interrupt_fields() {
        // identity = 0x2AB, priority = 0x1
        let bits: u32 = (0x2AB << 16) | 0x1;
        let claimi = ClaimInterrupt(bits);
        assert_eq!(claimi.interrupt_identity(), 0x2AB);
        assert_eq!(claimi.priority(), 0x1);
        assert!(claimi.is_pending());
    }

    #[test]
    fn test_claim_interrupt_none() {
        let claimi = ClaimInterrupt::NONE;
        assert_eq!(claimi.interrupt_identity(), 0);
        assert_eq!(claimi.priority(), 0);
        assert!(!claimi.is_pending());
    }
}

#[cfg(test)]
mod idc_accessor_tests {
    use super::{Aplic, Idc, InterruptDelivery};

    #[test]
    fn test_aplic_idc_accessor_offset() {
        let mut region = [0u32; (0x4000 + 0x20 * 4) / 4];
        let aplic = unsafe { &mut *(region.as_mut_ptr() as *mut Aplic) };
        let base = aplic as *const Aplic as usize;

        for (hart_index, expected_offset) in
            [(0usize, 0x4000usize), (1, 0x4020), (2, 0x4040), (3, 0x4060)]
        {
            let idc = aplic.idc(hart_index);
            assert_eq!(idc as *const Idc as usize - base, expected_offset);
        }
    }

    #[test]
    fn test_aplic_idc_accessor_reference() {
        let mut region = [0u32; (0x4000 + 0x20) / 4];
        let aplic = unsafe { &mut *(region.as_mut_ptr() as *mut Aplic) };
        let idc = aplic.idc(0);

        unsafe {
            idc.idelivery.write(InterruptDelivery::ENABLED);
            let idelivery_ptr = &idc.idelivery as *const _ as *const u32;
            assert_eq!(idelivery_ptr.read_volatile(), 1);
        }
    }
}
