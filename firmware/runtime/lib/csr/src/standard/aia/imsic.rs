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
///
/// See <https://docs.riscv.org/reference/aia/v1.0/IMSIC.html>.
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

    /// Writes this register in the current hart's machine interrupt file.
    ///
    /// Returns `None` when the selector is invalid for the current XLEN.
    /// Access to an unimplemented register can trap.
    /// On success, `miselect` remains set to this register's selector.
    ///
    /// # Safety
    ///
    /// Execute in M-mode with an accessible machine interrupt file. The value
    /// must be valid for the selected register. Serialize the selector and data
    /// accesses with all other indirect accesses on this hart, including
    /// interrupt handlers.
    #[inline]
    pub unsafe fn write_machine(self, value: usize) -> Option<()> {
        let selector = self.selector()?;
        // SAFETY: The caller establishes access, a valid value, and exclusive
        // use of miselect for the selector and data writes.
        unsafe {
            crate::value::write::<{ super::machine::Miselect::NUMBER }>(selector);
            crate::value::write::<{ super::machine::Mireg::NUMBER }>(value);
        }
        Some(())
    }
}
