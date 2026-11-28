//! Runtime support for RustSBI firmware.
//!
//! [`PlatformDescription`] validates the device tree received at firmware
//! entry.
//! Policy firmware may inspect that description, while [`memory`] derives
//! physical-memory access from it.

#![no_std]
#![warn(missing_docs)]
#![cfg_attr(
    any(target_arch = "riscv32", target_arch = "riscv64"),
    feature(riscv_ext_intrinsics)
)]

extern crate alloc;

pub mod console;
mod device_tree;
mod error;
pub mod events;
pub mod fence;
pub mod heap;
pub mod memory;
pub mod reset;
pub mod trap;

cfg_select! {
    any(target_arch = "riscv32", target_arch = "riscv64") => {
        pub mod boot;
        pub mod debug;
        pub mod fwft;
        pub mod hart;
        pub mod instruction;
        pub mod ipi;
        mod irq;
        pub mod pmp;
        pub mod pmu;
        mod recovery;
        pub mod soc;
        pub mod timer;
    }
    _ => {}
}

pub use error::{Error, Result};

pub use device_tree::{DeviceTreeHandoff, PlatformDescription, PlatformView, node_is_enabled};
/// Read-only Flattened Devicetree types used by firmware policy.
pub use fdt::{Fdt, node::FdtNode, standard_nodes::Compatible};
pub use macros::{entry, sbi_policy};
/// The original RustSBI library, re-exported under its own name.
///
/// Firmware policy crates receive RustSBI transitively through this crate and
/// refer to it as `runtime::rustsbi`.
pub use rustsbi;
