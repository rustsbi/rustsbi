//! Rust support for RISC-V Advanced Interrupt Architecture (AIA).
//!
//! This crate follows _The RISC-V Advanced Interrupt Architecture_ specification, Version 1.0, Revised 20250312.  

#![no_std]

pub mod geilen;
pub mod peripheral;
pub mod register;

use core::num::NonZeroU16;

/// RISC-V AIA Interrupt Identity (IID).
///
/// An IID is the encoded identity used by AIA/IMSIC to refer to an interrupt.
/// Value `0` is reserved/invalid. Valid identities are in the range `1..=N`.
/// The specification allows a platform-chosen `N` drawn from {63, 127, ..., 2047}
/// (i.e., one less than a multiple of 64). This implementation conservatively
/// assumes `N == 2047` unless a smaller limit is enforced elsewhere.
///
/// # Examples
///
/// ```
/// # use riscv_aia::Iid;
/// assert!(Iid::new(1).is_some());
/// assert!(Iid::new(2047).is_some());
/// assert!(Iid::new(0).is_none());
/// assert!(Iid::new(3000).is_none());
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Iid {
    number: NonZeroU16,
}

impl Iid {
    /// Attempts to construct an [`Iid`] from `number`.
    ///
    /// Returns `Some(Iid)` when `1 <= number <= 2047`; returns `None` if
    /// `number` is `0` or exceeds the assumed maximum.
    #[inline]
    pub const fn new(number: u16) -> Option<Iid> {
        // Note: 2047 chosen for default software cap; platform may choose a smaller N
        const IID_MAX: u16 = 2047;
        // TODO: use Option filter-map once stablized in Rust's std.
        match number {
            1..=IID_MAX => match NonZeroU16::new(number) {
                Some(nz) => Some(Iid { number: nz }),
                None => None, // only hits when number == 0; kept to avoid unwraps in const
            },
            _ => None,
        }
    }

    /// Returns the underlying interrupt identity number as `u16`.
    #[inline]
    pub const fn number(self) -> u16 {
        self.number.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iid_new_bounds() {
        assert!(Iid::new(0).is_none());
        assert!(Iid::new(1).is_some());
        assert!(Iid::new(2047).is_some());
        assert!(Iid::new(2048).is_none());
    }
}
