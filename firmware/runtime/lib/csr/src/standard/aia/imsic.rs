//! IMSIC interrupt identities and indirect registers.

use core::num::NonZeroU16;

/// A nonzero IMSIC interrupt identity in the architectural range `1..=2047`.
///
/// A particular interrupt file may implement fewer identities; its capacity
/// must be checked separately.
/// See <https://docs.riscv.org/reference/aia/v1.0/IMSIC.html>.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct Iid(NonZeroU16);

impl Iid {
    /// Returns an interrupt identity if `number` is in `1..=2047`.
    #[inline]
    pub const fn new(number: u16) -> Option<Self> {
        if number > 0x7ff {
            return None;
        }
        match NonZeroU16::new(number) {
            Some(number) => Some(Self(number)),
            None => None,
        }
    }

    /// Returns the interrupt identity number.
    #[inline]
    pub const fn number(self) -> u16 {
        self.0.get()
    }
}

/// An IMSIC indirect register, distinct from the `miselect` and `mireg` CSRs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImsicRegister {
    /// Interrupt delivery enable.
    Delivery,
    /// Interrupt priority threshold.
    Threshold,
    /// A pending-bit array register.
    Pending(u8),
    /// An enable-bit array register.
    Enable(u8),
}

impl ImsicRegister {
    /// Returns the architectural selector for this register.
    ///
    /// Returns `None` for indices outside 0 through 63 and for odd indices on
    /// RV64. A valid selector does not guarantee that the register exists on
    /// the current hart.
    ///
    /// See <https://docs.riscv.org/reference/aia/v1.0/IMSIC.html>.
    #[inline]
    pub const fn selector(self) -> Option<usize> {
        let (base, index) = match self {
            Self::Delivery => return Some(0x70),
            Self::Threshold => return Some(0x72),
            Self::Pending(index) => (0x80, index),
            Self::Enable(index) => (0xc0, index),
        };
        if index >= 64 || (cfg!(target_pointer_width = "64") && index % 2 != 0) {
            None
        } else {
            Some(base + index as usize)
        }
    }
}
