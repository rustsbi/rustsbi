//! Runtime mechanisms for RustSBI policy firmware.
//!
//! The [`memory`] module turns trusted physical-resource descriptions into
//! bounded handles for supervisor RAM and MMIO.
//! Board discovery and resource policy remain in the consuming firmware.

#![no_std]
#![warn(missing_docs)]

extern crate alloc;

pub mod memory;

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

/// A result returned by a Runtime operation.
pub type Result<T> = core::result::Result<T, Error>;
