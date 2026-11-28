//! Trap payloads, causes, and cause-code delegation sets.
//!
//! The [cause code](https://docs.riscv.org/reference/isa/v20260120/priv/machine.html#mcause)
//! identifies the corresponding bit in a [delegation register](https://docs.riscv.org/reference/isa/v20260120/priv/machine.html#2-1-1-8-machine-trap-delegation-medeleg-and-mideleg-registers).

use super::xlen::Xlen;

csr_value! {
    /// Raw trap-value register operand (`mtval`, `stval`, `htval`, `vstval`, `mtval2`).
    ///
    /// Reference: <https://docs.riscv.org/reference/isa/v20260120/priv/machine.html#2-1-1-16-machine-trap-value-mtval-register>.
    pub struct TrapValueBits;

    /// Raw transformed trap-instruction register operand (`mtinst`, `htinst`).
    ///
    /// Reference: <https://docs.riscv.org/reference/isa/v20260120/priv/hypervisor.html#14-1-4-5-machine-trap-instruction-mtinst-register>.
    pub struct TrapInstructionBits;

    /// Trap cause register word (`mcause`, `scause`, `vscause`).
    pub struct CauseValue;

    /// Interrupt cause-code set for delegation and interrupt enables.
    pub struct InterruptSet;
}

csr_value! {
    /// Synchronous exceptions delegated by the 64-bit `medeleg` and `hedeleg` registers.
    pub struct ExceptionSet(u64);
}

/// Decoded interrupt or synchronous exception cause (`mcause`, `scause`, `vscause`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrapCause {
    /// Interrupt bit: `1`.
    Interrupt(Interrupt),
    /// Interrupt bit: `0`.
    Exception(Exception),
}

macro_rules! cause_codes {
    (
        $(#[$meta:meta])*
        pub enum $Name:ident {
            $($(#[$variant_meta:meta])* $Variant:ident = $code:literal,)*
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub enum $Name {
            $(
                $(#[$variant_meta])*
                #[doc = concat!("Cause code: `", stringify!($code), "`.")]
                $Variant,
            )*
            /// Reserved, custom, or otherwise unrecognized cause code.
            Unknown(usize),
        }

        impl $Name {
            /// Decodes a cause code, preserving every unrecognized value.
            #[inline(always)]
            pub const fn from_code(code: usize) -> Self {
                match code {
                    $($code => Self::$Variant,)*
                    raw => Self::Unknown(raw),
                }
            }

            /// Returns the architectural cause code.
            #[inline(always)]
            pub const fn code(self) -> usize {
                match self {
                    $(Self::$Variant => $code,)*
                    Self::Unknown(raw) => raw,
                }
            }
        }
    };
}

cause_codes! {
    /// Architectural interrupt cause codes, preserving unknown/custom codes.
    ///
    /// References: [H extension](https://docs.riscv.org/reference/isa/v20260120/priv/hypervisor.html#hinterruptregs)
    /// and [AIA major interrupt identities](https://docs.riscv.org/reference/aia/v1.0/MSLevel.html).
    pub enum Interrupt {
        SupervisorSoftware = 1,
        VirtualSupervisorSoftware = 2,
        MachineSoftware = 3,
        SupervisorTimer = 5,
        VirtualSupervisorTimer = 6,
        MachineTimer = 7,
        SupervisorExternal = 9,
        VirtualSupervisorExternal = 10,
        MachineExternal = 11,
        SupervisorGuestExternal = 12,
        CounterOverflow = 13,
        LowPriorityRasEvent = 35,
        HighPriorityRasEvent = 43,
    }
}

cause_codes! {
    /// Architectural synchronous exception cause codes, preserving unknown/custom codes.
    ///
    /// Store variants also cover AMO instructions.
    pub enum Exception {
        InstructionMisaligned = 0,
        InstructionAccess = 1,
        IllegalInstruction = 2,
        Breakpoint = 3,
        LoadMisaligned = 4,
        LoadAccess = 5,
        StoreMisaligned = 6,
        StoreAccess = 7,
        /// Also raised by VU-mode.
        UserEcall = 8,
        /// Also raised by HS-mode.
        SupervisorEcall = 9,
        VirtualSupervisorEcall = 10,
        MachineEcall = 11,
        InstructionPage = 12,
        LoadPage = 13,
        StorePage = 15,
        DoubleTrap = 16,
        /// Raised by `Zicfilp` or `Zicfiss`.
        SoftwareCheck = 18,
        HardwareError = 19,
        InstructionGuestPage = 20,
        LoadGuestPage = 21,
        VirtualInstruction = 22,
        StoreGuestPage = 23,
    }
}

impl CauseValue {
    const fn interrupt_bit(xlen: Xlen) -> Option<usize> {
        let width = xlen.bytes() as u32 * u8::BITS;
        1usize.checked_shl(width - 1)
    }

    /// Decodes the interrupt bit and cause code using the source CSR's effective XLEN.
    ///
    /// # Panics
    ///
    /// Panics if `xlen` exceeds the width of this program's integer word.
    #[inline(always)]
    pub const fn cause(self, xlen: Xlen) -> TrapCause {
        let interrupt_bit =
            Self::interrupt_bit(xlen).expect("source CSR width exceeds native word");
        let code = self.0 & (interrupt_bit - 1);
        if (self.0 & interrupt_bit) != 0 {
            TrapCause::Interrupt(Interrupt::from_code(code))
        } else {
            TrapCause::Exception(Exception::from_code(code))
        }
    }

    /// Encodes a cause for the destination CSR's effective XLEN.
    ///
    /// Returns `None` if the code or requested width does not fit.
    #[inline(always)]
    pub const fn from_cause(cause: TrapCause, xlen: Xlen) -> Option<Self> {
        let Some(interrupt_bit) = Self::interrupt_bit(xlen) else {
            return None;
        };
        let (flag, code) = match cause {
            TrapCause::Exception(e) => (0, e.code()),
            TrapCause::Interrupt(i) => (interrupt_bit, i.code()),
        };
        if code < interrupt_bit {
            Some(Self(flag | code))
        } else {
            None
        }
    }
}

impl ExceptionSet {
    /// Sets or clears the indicated exception bit in this local mask.
    ///
    /// # Panics
    ///
    /// Panics if the exception code is 64 or greater.
    #[inline(always)]
    pub const fn set(&mut self, exception: Exception, delegated: bool) {
        let code = exception.code();
        assert!(
            code < u64::BITS as usize,
            "exception exceeds delegation width"
        );
        let bit = 1u64 << code;
        if delegated {
            self.0 |= bit;
        } else {
            self.0 &= !bit;
        }
    }

    /// Reports whether `exception` is set in this mask.
    #[inline(always)]
    pub const fn contains(self, exception: Exception) -> bool {
        let code = exception.code();
        code < u64::BITS as usize && (self.0 & (1u64 << code)) != 0
    }
}
