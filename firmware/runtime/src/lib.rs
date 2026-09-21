//! Runtime support for RustSBI firmware.
//!
//! [`PlatformDescription`] validates the device tree received at firmware
//! entry. Policy firmware may inspect that description, while [`memory`]
//! derives physical-memory access from it.

#![no_std]
#![warn(missing_docs)]

extern crate alloc;

use core::fmt;

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
pub mod boot;
pub mod cfg;
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
pub mod csr;
mod device_tree;
pub mod events;
pub mod hart;
pub mod ipi;
pub mod irq;
pub mod machine_irq;
pub mod memory;
pub mod soc;
mod spacemit_k1;
mod sunxi_rtc_v203;
pub mod timer;
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
pub mod trap;

pub use device_tree::{DeviceTreeHandoff, PlatformDescription, PlatformView, node_is_enabled};
/// The original RustSBI library, re-exported under its own name.
///
/// Firmware policy crates receive RustSBI transitively through this crate
/// and refer to it as `runtime::rustsbi`.
pub use rustsbi;
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
