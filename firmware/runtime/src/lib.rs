//! Runtime support for RustSBI firmware.
//!
//! [`PlatformDescription`] validates the device tree received at firmware
//! entry.
//! Policy firmware may inspect that description, while [`memory`] derives
//! physical-memory access from it.

#![no_std]
#![warn(missing_docs)]

extern crate alloc;

pub mod boot;
pub mod debug;
mod device_tree;
mod error;
pub mod fwft;
pub mod hart;
pub mod heap;
pub mod ipi;
mod irq;
pub mod machine_irq;
pub mod memory;
pub mod pmu;
mod recovery;
pub mod soc;
mod spacemit_k1;
mod sunxi_rtc_v203;
pub mod timer;
pub mod trap;

pub use error::{Error, Result};

pub use device_tree::{DeviceTreeHandoff, PlatformDescription, PlatformView, node_is_enabled};
/// Read-only Flattened Devicetree types used by firmware policy.
pub use fdt::{Fdt, node::FdtNode, standard_nodes::Compatible};
pub use macros::entry;
/// The original RustSBI library, re-exported under its own name.
///
/// Firmware policy crates receive RustSBI transitively through this crate and
/// refer to it as `runtime::rustsbi`.
pub use rustsbi;
pub use spacemit_k1::SpacemitK1Registers;
