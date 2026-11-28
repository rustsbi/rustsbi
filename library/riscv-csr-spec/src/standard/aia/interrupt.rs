//! Major interrupt identities and top-interrupt CSR values.

use crate::standard::isa::values::trap::Interrupt;
use crate::{ReadFieldValue, WriteFieldValue};

/// 12-bit major interrupt identity (`0..=4095`) used by `*topi` and `hvictl`.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MajorIid(u16);

impl MajorIid {
    /// Largest major interrupt identity.
    pub const MAX_NUMBER: u16 = 0x0fff;

    /// Returns a [`MajorIid`] if `number <= 4095`.
    #[inline(always)]
    pub const fn new(number: u16) -> Option<Self> {
        if number <= Self::MAX_NUMBER {
            Some(Self(number))
        } else {
            None
        }
    }

    /// Returns the architectural major interrupt number.
    #[inline(always)]
    pub const fn number(self) -> u16 {
        self.0
    }
}

impl TryFrom<Interrupt> for MajorIid {
    type Error = ();

    #[inline(always)]
    fn try_from(value: Interrupt) -> Result<Self, ()> {
        let code = value.code();
        if code <= Self::MAX_NUMBER as usize {
            Ok(Self(code as u16))
        } else {
            Err(())
        }
    }
}

impl From<MajorIid> for Interrupt {
    #[inline(always)]
    fn from(value: MajorIid) -> Self {
        Self::from_code(value.0 as usize)
    }
}

impl ReadFieldValue for MajorIid {
    #[inline(always)]
    fn from_field(raw: usize) -> Self {
        Self((raw & Self::MAX_NUMBER as usize) as u16)
    }
}
impl WriteFieldValue for MajorIid {
    #[inline(always)]
    fn into_field(self) -> usize {
        self.0 as usize
    }
}

csr_value! {
    /// Top major interrupt CSR value (`mtopi`, `stopi`, `vstopi`).
    pub struct TopInterrupt(usize);
}

impl TopInterrupt {
    /// Returns the major interrupt identity, or `None` if the entire report is zero.
    #[inline(always)]
    pub const fn iid(self) -> Option<MajorIid> {
        if self.0 == 0 {
            None
        } else {
            MajorIid::new(((self.0 >> 16) & 0x0fff) as u16)
        }
    }

    /// Extracts the priority number (`bits[7:0]`).
    #[inline(always)]
    pub const fn iprio(self) -> u8 {
        (self.0 & 0xff) as u8
    }
}
