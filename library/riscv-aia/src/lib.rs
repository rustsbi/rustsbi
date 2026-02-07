//! Rust support for RISC-V Advanced Interrupt Architecture (AIA).
//!
//! This crate follows _The RISC-V Advanced Interrupt Architecture_ specification, Version 1.0, Revised 20250312.  

#![no_std]

pub mod geilen;
pub mod peripheral;
pub mod register;

use core::num::NonZeroU16;

use riscv::InterruptNumber;

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
///
/// ```
/// # use riscv_aia::Iid;
/// # mod mtopi {
/// # use riscv_aia::Iid;
/// # pub struct Mtopi;
/// # impl Mtopi { pub fn iid(&self) -> Option<Iid> { None } }
/// # pub fn read() -> Mtopi { Mtopi }
/// # }
/// // Read AIA interrupt ID from `mtopi` register.
/// // On each read, `mtopi` returns the next interrupt we should process.
/// while let Some(iid) = mtopi::read().iid() {
///     if iid == Iid::MTIMER {
///         // Handle machine timer interrupt in SBI implementation.
///     } else if iid == Iid::MSOFT {
///         // Handle machine software interrupt in SBI implementation.
///     } else if iid == Iid::MEXT {
///         // Handle machine external interrupt in SBI implementation.
///     } else {
///         // Default interrupt handler.
///     }
/// }
/// ```
///
/// `Iid` can be converted into, or can be tried to convert from the `Interrupt` enum
/// in the `riscv` crate:
///
/// ```
/// # use riscv_aia::Iid;
/// use riscv::interrupt::Interrupt;
///
/// let interrupt = Interrupt::MachineSoft;
/// assert_eq!(Iid::MSOFT, interrupt.into());
///
/// let iid = Iid::MEXT;
/// assert_eq!(Ok(Interrupt::MachineExternal), iid.try_into());
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Iid {
    number: NonZeroU16,
}

impl Iid {
    /// `Iid` for Supervisor software interrupt in standard RISC-V.
    pub const SSOFT: Iid = Iid::new(1).unwrap();

    /// `Iid` for Machine software interrupt in standard RISC-V.
    pub const MSOFT: Iid = Iid::new(3).unwrap();

    /// `Iid` for Supervisor timer interrupt in standard RISC-V.
    pub const STIMER: Iid = Iid::new(5).unwrap();

    /// `Iid` for Machine timer interrupt in standard RISC-V.
    pub const MTIMER: Iid = Iid::new(7).unwrap();

    /// `Iid` for Supervisor external interrupt in standard RISC-V.
    pub const SEXT: Iid = Iid::new(9).unwrap();

    /// `Iid` for Machine external interrupt in standard RISC-V.
    pub const MEXT: Iid = Iid::new(11).unwrap();

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

impl From<riscv::interrupt::Interrupt> for Iid {
    #[inline]
    fn from(value: riscv::interrupt::Interrupt) -> Self {
        assert!(value.number() <= u16::MAX as usize && value.number() != 0);
        Iid::new(value.number() as u16).unwrap()
    }
}

impl TryFrom<Iid> for riscv::interrupt::Interrupt {
    type Error = ();

    #[inline]
    fn try_from(value: Iid) -> Result<Self, Self::Error> {
        use riscv::interrupt::Interrupt;
        match value {
            Iid::SSOFT => Ok(Interrupt::SupervisorSoft),
            Iid::MSOFT => Ok(Interrupt::MachineSoft),
            Iid::STIMER => Ok(Interrupt::SupervisorTimer),
            Iid::MTIMER => Ok(Interrupt::MachineTimer),
            Iid::SEXT => Ok(Interrupt::SupervisorExternal),
            Iid::MEXT => Ok(Interrupt::MachineExternal),
            _ => Err(()),
        }
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

    #[test]
    fn iid_consts() {
        assert_eq!(Iid::SSOFT.number(), 1);
        assert_eq!(Iid::MSOFT.number(), 3);
        assert_eq!(Iid::STIMER.number(), 5);
        assert_eq!(Iid::MTIMER.number(), 7);
        assert_eq!(Iid::SEXT.number(), 9);
        assert_eq!(Iid::MEXT.number(), 11);
    }

    #[test]
    fn iid_usage_match_if() {
        let iid = Iid::MEXT;
        if iid == Iid::MTIMER {
            unreachable!()
        } else if iid == Iid::MSOFT {
            unreachable!()
        } else if iid == Iid::MEXT {
            assert!(true)
        } else {
            unreachable!()
        }

        match iid {
            Iid::MTIMER => unreachable!(),
            Iid::MSOFT => unreachable!(),
            Iid::MEXT => assert!(true),
            _ => unreachable!(),
        }

        // Mock `mtopi::read` result where `iid: Option<Iid>`.
        let iid = Some(Iid::MSOFT);
        match iid {
            Some(Iid::MTIMER) => unreachable!(),
            Some(Iid::MSOFT) => assert!(true),
            Some(Iid::MEXT) => unreachable!(),
            // Redundant, can be `_ => unreachable!()` in real use.
            Some(_) | None => unreachable!(),
        }
    }

    #[test]
    fn iid_convert_riscv_crate() {
        use riscv::interrupt::Interrupt;
        let irqs = [
            (Interrupt::SupervisorExternal, Iid::SEXT),
            (Interrupt::MachineExternal, Iid::MEXT),
            (Interrupt::SupervisorSoft, Iid::SSOFT),
            (Interrupt::MachineSoft, Iid::MSOFT),
            (Interrupt::SupervisorTimer, Iid::STIMER),
            (Interrupt::MachineTimer, Iid::MTIMER),
        ];
        for (riscv_irq, aia_iid) in irqs {
            assert_eq!(aia_iid, Iid::from(riscv_irq));
            assert_eq!(aia_iid, riscv_irq.into());
            assert_eq!(aia_iid.try_into(), Ok(riscv_irq));
            assert_eq!(Interrupt::try_from(aia_iid), Ok(riscv_irq));
        }
    }
}
