//! [IMSIC](https://docs.riscv.org/reference/aia/v1.0/IMSIC.html) interrupt identities, selectors and external-interrupt values.

use core::num::NonZeroU16;

use crate::standard::isa::values::xlen::Xlen;

/// Nonzero IMSIC minor interrupt identity in the architectural range `1..=2047`.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Iid(NonZeroU16);

impl Iid {
    /// Largest minor interrupt identity defined by AIA.
    pub const MAX_NUMBER: u16 = 0x7ff;

    /// Returns an interrupt identity if `number` is in `1..=2047`.
    #[inline(always)]
    pub const fn new(number: u16) -> Option<Self> {
        if number > Self::MAX_NUMBER {
            return None;
        }
        match NonZeroU16::new(number) {
            Some(nz) => Some(Self(nz)),
            None => None,
        }
    }

    /// Returns the numeric minor interrupt identity (`1..=2047`).
    #[inline(always)]
    pub const fn number(self) -> u16 {
        self.0.get()
    }
}

/// Indirect IMSIC register selectors accessed via `miselect`/`siselect`/`vsiselect`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImsicRegister {
    /// External interrupt delivery enable (`eidelivery`, selector `0x70`).
    Delivery,
    /// External interrupt priority threshold (`eithreshold`, selector `0x72`).
    Threshold,
    /// Pending bit array `eip<k>` (`0x80 + k`, `k < 64`, even `k` on RV64).
    Pending(u8),
    /// Enable bit array `eie<k>` (`0xc0 + k`, `k < 64`, even `k` on RV64).
    Enable(u8),
}

impl ImsicRegister {
    /// Returns the selector if its array index is valid at the specified XLEN.
    #[inline(always)]
    pub const fn selector_for_xlen(self, xlen: Xlen) -> Option<usize> {
        let (base, index) = match self {
            Self::Delivery => return Some(0x70),
            Self::Threshold => return Some(0x72),
            Self::Pending(index) => (0x80, index),
            Self::Enable(index) => (0xc0, index),
        };
        if index >= 64 || (matches!(xlen, Xlen::X64) && index % 2 != 0) {
            None
        } else {
            Some(base + index as usize)
        }
    }

    /// Returns the architectural selector for `usize::BITS`.
    #[inline(always)]
    pub const fn selector(self) -> Option<usize> {
        self.selector_for_xlen(Xlen::NATIVE)
    }
}

/// Values for the IMSIC `eidelivery` register.
pub struct Eidelivery;

impl Eidelivery {
    /// The setting that disables external-interrupt delivery.
    pub const DISABLED: usize = 0;
    /// Delivery from the local interrupt file.
    pub const ENABLED: usize = 1;
    /// Direct delivery from an attached PLIC or APLIC, if implemented.
    ///
    /// This setting is available only on machine and supervisor interrupt files.
    pub const FROM_APLIC: usize = 0x4000_0000;
}

/// Priority thresholds for the IMSIC `eithreshold` register.
pub struct Eithreshold;

impl Eithreshold {
    /// A threshold that admits every pending and enabled interrupt.
    pub const ALL: usize = 0;
}

csr_value! {
    /// Top external interrupt CSR value (`mtopei`, `stopei`, `vstopei`).
    ///
    /// Reference: <https://docs.riscv.org/reference/aia/v1.0/IMSIC.html#2-1-9-top-external-interrupt-csrs-mtopei-stopei-vstopei>.
    pub struct TopExternalInterrupt(usize);
}

impl TopExternalInterrupt {
    const FIELD_MASK: usize = 0x07ff;
    const IDENTITY_SHIFT: u32 = 16;

    /// Returns the minor interrupt identity, or `None` if the identity field is zero.
    #[inline(always)]
    pub const fn iid(self) -> Option<Iid> {
        Iid::new(((self.0 >> Self::IDENTITY_SHIFT) & Self::FIELD_MASK) as u16)
    }

    /// Returns the reported interrupt priority, equal to the identity on an IMSIC.
    #[inline(always)]
    pub const fn iprio(self) -> u16 {
        (self.0 & Self::FIELD_MASK) as u16
    }
}
