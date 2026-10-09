//! Machine trap initialization, dispatch, emulation, and guarded recovery.
//!
//! Firmware publishes SBI policy and an optional [`AccessDispatcher`]. Runtime
//! retains the trap frame and instruction-emulation steps internally.

mod decode;
pub(crate) mod dispatch;
mod emulate;
pub(crate) mod entry;
pub(crate) mod frame;
pub(crate) mod init;
mod recovery;
mod redirect;

pub use decode::ValueKind;
pub use init::{AccessDispatcher, AccessError, InitError, init, install_access_dispatcher};
pub(crate) use recovery::{
    read_boot_byte_guarded, read_csr_guarded, sfence_vma_guarded, swap_csr_guarded,
    write_csr_guarded,
};

use core::fmt;

/// The current hart's live machine trap facts for diagnostic reporting.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiagnosticSnapshot {
    /// Machine interrupt or exception cause, retaining its architectural code.
    pub cause: riscv::interrupt::Trap<usize, usize>,
    /// Instruction address recorded by the most recent machine trap.
    pub program_counter: usize,
    /// Additional trap information recorded by the hardware.
    pub trap_value: usize,
}

impl DiagnosticSnapshot {
    /// Captures this hart's live trap facts with machine interrupts masked.
    pub fn capture() -> Self {
        use crate::csr::{Mcause, Mepc, Mtval, Readable};
        riscv::interrupt::machine::free(|| {
            let bits = Mcause::read().expect("machine cause CSR read failed");
            Self {
                cause: riscv::register::mcause::Mcause::from_bits(bits).cause(),
                program_counter: Mepc::read().expect("machine exception PC CSR read failed"),
                trap_value: Mtval::read().expect("machine trap value CSR read failed"),
            }
        })
    }
}

/// An error from a guarded machine operation or trap emulation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    /// The instruction is not emulated — unknown, or its write-back target
    /// is unavailable.
    /// The caller should redirect.
    UnsupportedInstruction,
    /// A guarded instruction fetch or data access faulted.
    /// `cause` and `tval` retain the recovered fault's exception and address;
    /// emulation callers redirect with these secondary facts.
    MemoryFault {
        /// The recovered fault's `mcause`.
        cause: usize,
        /// The recovered fault's `mtval`.
        tval: usize,
    },
    /// The trap originated from M-mode; there is no lower-privilege owner to
    /// receive a redirect.
    /// The caller should fail.
    MachineOrigin,
}

impl From<AccessError> for Error {
    fn from(_: AccessError) -> Self {
        Self::UnsupportedInstruction
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Error::UnsupportedInstruction => "unsupported trapped instruction",
            Error::MemoryFault { .. } => "trapped access faulted",
            Error::MachineOrigin => "trap originated from M-mode",
        })
    }
}
