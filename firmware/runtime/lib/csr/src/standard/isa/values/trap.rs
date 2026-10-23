//! Trap causes and delegation sets.
use crate::macros::csr;

csr! {
    /// Exception delegation bits indexed by synchronous cause code.
    pub struct ExceptionSet(usize);
    /// Interrupt delegation bits indexed by interrupt cause code.
    pub struct InterruptSet(usize);
    /// Raw trap cause with the interrupt flag in the most significant bit.
    pub struct CauseValue(usize);
}

/// A decoded interrupt or synchronous exception.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrapCause {
    Interrupt(Interrupt),
    Exception(Exception),
}

/// Interrupt cause codes, preserving unknown values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Interrupt {
    SupervisorSoftware,
    MachineSoftware,
    SupervisorTimer,
    MachineTimer,
    SupervisorExternal,
    MachineExternal,
    SupervisorGuestExternal,
    CounterOverflow,
    Unknown(usize),
}

/// Synchronous exception cause codes, preserving unknown values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Exception {
    InstructionMisaligned,
    InstructionAccess,
    IllegalInstruction,
    Breakpoint,
    LoadMisaligned,
    LoadAccess,
    StoreMisaligned,
    StoreAccess,
    UserEcall,
    SupervisorEcall,
    VirtualSupervisorEcall,
    MachineEcall,
    InstructionPage,
    LoadPage,
    StorePage,
    DoubleTrap,
    SoftwareCheck,
    HardwareError,
    InstructionGuestPage,
    LoadGuestPage,
    VirtualInstruction,
    StoreGuestPage,
    Unknown(usize),
}

impl CauseValue {
    /// Separates the interrupt flag from the cause code.
    pub fn cause(self) -> TrapCause {
        let interrupt = 1usize << (usize::BITS - 1);
        let code = self.0 & !interrupt;
        if self.0 & interrupt != 0 {
            TrapCause::Interrupt(match code {
                1 => Interrupt::SupervisorSoftware,
                3 => Interrupt::MachineSoftware,
                5 => Interrupt::SupervisorTimer,
                7 => Interrupt::MachineTimer,
                9 => Interrupt::SupervisorExternal,
                11 => Interrupt::MachineExternal,
                12 => Interrupt::SupervisorGuestExternal,
                13 => Interrupt::CounterOverflow,
                raw => Interrupt::Unknown(raw),
            })
        } else {
            TrapCause::Exception(match code {
                0 => Exception::InstructionMisaligned,
                1 => Exception::InstructionAccess,
                2 => Exception::IllegalInstruction,
                3 => Exception::Breakpoint,
                4 => Exception::LoadMisaligned,
                5 => Exception::LoadAccess,
                6 => Exception::StoreMisaligned,
                7 => Exception::StoreAccess,
                8 => Exception::UserEcall,
                9 => Exception::SupervisorEcall,
                10 => Exception::VirtualSupervisorEcall,
                11 => Exception::MachineEcall,
                12 => Exception::InstructionPage,
                13 => Exception::LoadPage,
                15 => Exception::StorePage,
                16 => Exception::DoubleTrap,
                18 => Exception::SoftwareCheck,
                19 => Exception::HardwareError,
                20 => Exception::InstructionGuestPage,
                21 => Exception::LoadGuestPage,
                22 => Exception::VirtualInstruction,
                23 => Exception::StoreGuestPage,
                raw => Exception::Unknown(raw),
            })
        }
    }
}

impl From<usize> for ExceptionSet {
    fn from(mask: usize) -> Self {
        Self(mask)
    }
}

impl From<usize> for InterruptSet {
    fn from(mask: usize) -> Self {
        Self(mask)
    }
}

impl From<usize> for CauseValue {
    fn from(bits: usize) -> Self {
        Self(bits)
    }
}

impl InterruptSet {
    pub const fn empty() -> Self {
        Self(0)
    }
}

impl Exception {
    fn code(self) -> usize {
        match self {
            Self::InstructionMisaligned => 0,
            Self::InstructionAccess => 1,
            Self::IllegalInstruction => 2,
            Self::Breakpoint => 3,
            Self::LoadMisaligned => 4,
            Self::LoadAccess => 5,
            Self::StoreMisaligned => 6,
            Self::StoreAccess => 7,
            Self::UserEcall => 8,
            Self::SupervisorEcall => 9,
            Self::VirtualSupervisorEcall => 10,
            Self::MachineEcall => 11,
            Self::InstructionPage => 12,
            Self::LoadPage => 13,
            Self::StorePage => 15,
            Self::DoubleTrap => 16,
            Self::SoftwareCheck => 18,
            Self::HardwareError => 19,
            Self::InstructionGuestPage => 20,
            Self::LoadGuestPage => 21,
            Self::VirtualInstruction => 22,
            Self::StoreGuestPage => 23,
            Self::Unknown(raw) => raw,
        }
    }
}

impl Interrupt {
    fn code(self) -> usize {
        match self {
            Self::SupervisorSoftware => 1,
            Self::MachineSoftware => 3,
            Self::SupervisorTimer => 5,
            Self::MachineTimer => 7,
            Self::SupervisorExternal => 9,
            Self::MachineExternal => 11,
            Self::SupervisorGuestExternal => 12,
            Self::CounterOverflow => 13,
            Self::Unknown(raw) => raw,
        }
    }
}

impl From<TrapCause> for CauseValue {
    fn from(cause: TrapCause) -> Self {
        let interrupt = 1usize << (usize::BITS - 1);
        let (flag, code) = match cause {
            TrapCause::Exception(exception) => (0, exception.code()),
            TrapCause::Interrupt(cause) => (interrupt, cause.code()),
        };
        assert!(code < interrupt, "trap cause exceeds its code field");
        Self(flag | code)
    }
}

impl ExceptionSet {
    /// Updates one delegation choice in this local value.
    pub fn set(&mut self, exception: Exception, delegated: bool) {
        assert!(exception.code() < usize::BITS as usize);
        let bit = 1usize << exception.code();
        if delegated {
            self.0 |= bit;
        } else {
            self.0 &= !bit;
        }
    }

    pub fn except(exceptions: &[Exception]) -> Self {
        let mut bits = usize::MAX;
        for exception in exceptions {
            if exception.code() < usize::BITS as usize {
                bits &= !(1usize << exception.code());
            }
        }
        Self(bits)
    }

    pub fn contains(self, exception: Exception) -> bool {
        exception.code() < usize::BITS as usize && self.0 & (1usize << exception.code()) != 0
    }
}
