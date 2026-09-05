//! Runtime support for RustSBI firmware.
//!
//! [`PlatformDescription`] validates the device tree received at firmware
//! entry. Policy firmware may inspect that description, while [`memory`]
//! derives physical-memory access from it.

#![no_std]
#![warn(missing_docs)]

extern crate alloc;

use core::fmt;

mod device_tree;
pub mod memory;
mod spacemit_k1;

pub use device_tree::{DeviceTreeHandoff, PlatformDescription, PlatformView, node_is_enabled};
pub use spacemit_k1::SpacemitK1Registers;

/// An error returned by a Runtime operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
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
            Self::InvalidArgs => "invalid argument",
            Self::AccessDenied => "access denied",
            Self::NotEnoughResources => "resource unavailable",
            Self::Overflow => "address overflow",
        })
    }
}

/// A result returned by a Runtime operation.
pub type Result<T> = core::result::Result<T, Error>;
