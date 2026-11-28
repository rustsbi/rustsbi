#![doc = include_str!("../README.md")]
#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

#[macro_use]
mod macros;
pub mod capability;
mod field;
mod register;
pub mod standard;
pub mod vendor;

pub use field::{Field, Mask};
#[doc(hidden)]
pub use field::{ReadFieldValue, WriteFieldValue};
pub use register::{
    ClaimSpec, Clear, Csr, ReadMode, ReadSpec, Set, StandardRead, Width, WriteMode, WriteOp,
    WriteSpec,
};
pub(crate) use register::{ScalarCodec, sealed};
