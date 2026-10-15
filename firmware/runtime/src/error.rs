//! Public failure codes shared by Runtime interfaces.

use core::fmt;

/// An error returned by a Runtime operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    /// The operation requires initialization that has not completed.
    NotReady,
    /// The resource already has its permanent initialization.
    AlreadyInitialized,
    /// The resource is reserved or currently borrowed.
    Busy,
    /// The operation is invalid in the resource's current state.
    InvalidState,
    /// The execution environment does not support the requested operation.
    NotSupported,
    /// A hardware device operation failed.
    IoError,
    /// An allocation request cannot be satisfied.
    NoMemory,
    /// A recoverable hardware access raised an illegal-instruction exception.
    IllegalInstruction,
    /// An argument is invalid for the requested operation.
    InvalidArgs,
    /// The caller does not have access to the requested resource.
    AccessDenied,
    /// The requested resource is unavailable.
    NotEnoughResources,
    /// Address arithmetic overflowed.
    Overflow,
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotReady => "runtime service is not ready",
            Self::AlreadyInitialized => "resource is already initialized",
            Self::Busy => "resource is busy",
            Self::InvalidState => "invalid resource state",
            Self::NotSupported => "operation is not supported",
            Self::IoError => "device operation failed",
            Self::NoMemory => "insufficient memory",
            Self::IllegalInstruction => "hardware access raised an illegal-instruction exception",
            Self::InvalidArgs => "invalid argument",
            Self::AccessDenied => "access denied",
            Self::NotEnoughResources => "resource unavailable",
            Self::Overflow => "address overflow",
        })
    }
}

/// A result returned by a Runtime operation.
pub type Result<T> = core::result::Result<T, Error>;
