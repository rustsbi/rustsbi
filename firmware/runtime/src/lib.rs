//! Runtime support for RustSBI firmware.
//!
//! Firmware calls semantic services such as [`timer`], [`pmu`], and [`pmp`].
//! Runtime owns their architectural operations and keeps numeric CSR access
//! private to the subsystem implementations.
//!
//! [`PlatformDescription`] validates the device tree received at firmware
//! entry. Policy firmware may inspect that description, while [`memory`]
//! derives physical-memory access from it.

#![no_std]
#![warn(missing_docs)]

extern crate alloc;

use core::fmt;

pub mod boot;
mod csr;
pub mod debug;
mod device_tree;
pub mod events;
pub mod features;
pub mod hart;
pub mod heap;
mod instructions;
pub mod ipi;
pub mod machine_irq;
pub mod memory;
pub mod pmp;
pub mod pmu;
pub mod rfence;
pub mod soc;
mod sunxi_rtc_v203;
pub mod timer;
pub mod trap;

pub use device_tree::{DeviceTreeHandoff, PlatformDescription, PlatformView, node_is_enabled};
/// Read-only Flattened Devicetree types used by firmware policy.
pub use fdt::{Fdt, node::FdtNode, standard_nodes::Compatible};
/// The original RustSBI library, re-exported under its own name.
///
/// Firmware policy crates receive RustSBI transitively through this crate
/// and refer to it as `runtime::rustsbi`.
pub use rustsbi;
pub use soc::spacemit::k1::SpacemitK1Registers;

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
    /// A one-time Runtime resource has already been initialized.
    AlreadyInitialized,
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidArgs => "invalid argument",
            Self::AccessDenied => "access denied",
            Self::NotEnoughResources => "resource unavailable",
            Self::Overflow => "address overflow",
            Self::AlreadyInitialized => "resource already initialized",
        })
    }
}

/// A result returned by a Runtime operation.
pub type Result<T> = core::result::Result<T, Error>;
