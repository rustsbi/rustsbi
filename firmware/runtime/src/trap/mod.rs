//! Trap mechanism: complete trapped-instruction operations and the
//! Runtime-owned machine trap entry, dispatch, and lifecycle.
//!
//! The safe surface exposes only complete operations whose inputs bind to
//! the hardware trap CSRs; the individual steps (fetch, decode, memory
//! access, write-back, `mepc` advance) stay internal, and the one real
//! [`TrapFrame`](frame::TrapFrame) never escapes the Runtime.

mod decode;
pub(crate) mod dispatch;
mod emulate;
pub(crate) mod entry;
pub(crate) mod frame;
pub(crate) mod init;
mod recovery;
mod redirect;

pub use decode::ValueKind;
pub use init::{
    AccessDispatcher, AccessError, InitError, has_sstc, init, install_access_dispatcher,
    misaligned_delegated, set_misaligned_delegation,
};
pub use recovery::{read_csr_guarded, swap_csr_guarded, write_csr_guarded};

use core::fmt;

/// An error from a trap operation. No operation panics on input reachable
/// from the Next Stage or from policy code; panics are reserved for
/// detected firmware bugs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    /// The instruction is not emulated — unknown, or its write-back target
    /// is unavailable. The caller should redirect.
    UnsupportedInstruction,
    /// Fetch or data access faulted under the trapped context's privilege,
    /// carrying the recovered fault's facts: `cause` is the precise
    /// secondary exception to deliver, and `tval` the actually failing
    /// address. The caller should redirect with these facts.
    MemoryFault {
        /// The recovered fault's `mcause`.
        cause: usize,
        /// The recovered fault's `mtval`.
        tval: usize,
    },
    /// The trap originated from M-mode; there is no lower-privilege owner to
    /// receive a redirect. The caller should fail.
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
