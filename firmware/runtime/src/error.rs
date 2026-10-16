//! Public failure codes shared by Runtime interfaces.

use core::fmt;
use rustsbi::SbiRet;

/// An error returned by a Runtime operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    /// The operation requires initialization that has not completed.
    NotReady,
    /// The resource already has its permanent initialization.
    AlreadyInitialized,
    /// The resource has already started.
    AlreadyStarted,
    /// The resource has already stopped.
    AlreadyStopped,
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
    /// The operation requires a shared-memory area that has not been registered.
    NoSharedMemory,
    /// A recoverable hardware access raised an illegal-instruction exception.
    IllegalInstruction,
    /// An argument is invalid for the requested operation.
    InvalidArgs,
    /// The caller does not have access to the requested resource.
    AccessDenied,
    /// The configuration is locked against further changes.
    Locked,
    /// The requested resource is unavailable.
    NotEnoughResources,
    /// Arithmetic overflowed the representable range.
    Overflow,
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotReady => "runtime service is not ready",
            Self::AlreadyInitialized => "resource is already initialized",
            Self::AlreadyStarted => "resource is already started",
            Self::AlreadyStopped => "resource is already stopped",
            Self::Busy => "resource is busy",
            Self::InvalidState => "invalid resource state",
            Self::NotSupported => "operation is not supported",
            Self::IoError => "device operation failed",
            Self::NoMemory => "insufficient memory",
            Self::NoSharedMemory => "shared memory is not registered",
            Self::IllegalInstruction => "hardware access raised an illegal-instruction exception",
            Self::InvalidArgs => "invalid argument",
            Self::AccessDenied => "access denied",
            Self::Locked => "configuration is locked",
            Self::NotEnoughResources => "resource unavailable",
            Self::Overflow => "arithmetic overflow",
        })
    }
}

impl From<Error> for SbiRet {
    fn from(error: Error) -> Self {
        match error {
            Error::InvalidArgs => Self::invalid_param(),
            Error::NotSupported => Self::not_supported(),
            Error::AccessDenied => Self::denied(),
            Error::Locked => Self::denied_locked(),
            Error::AlreadyStarted => Self::already_started(),
            Error::AlreadyStopped => Self::already_stopped(),
            Error::NoSharedMemory => Self::no_shmem(),
            Error::NotReady
            | Error::AlreadyInitialized
            | Error::Busy
            | Error::InvalidState
            | Error::IoError
            | Error::NoMemory
            | Error::IllegalInstruction
            | Error::NotEnoughResources
            | Error::Overflow => Self::failed(),
        }
    }
}

/// A result that defaults to Runtime's error type.
pub type Result<T, E = Error> = core::result::Result<T, E>;
