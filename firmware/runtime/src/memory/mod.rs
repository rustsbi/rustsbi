//! Access to physical memory described by the boot device tree.
//!
//! [`crate::PlatformDescription::into_memory_resources`] returns supervisor RAM with reserved
//! memory and the firmware image excluded, plus a [`MemoryRegistry`] from
//! which non-overlapping device register windows can be acquired.
//! [`HandoffBuffer`] holds linked bytes whose ownership passes to the next
//! stage without retaining a Rust reference to their contents.

mod address;
mod handoff;
mod image;
mod mmio;
mod region;
mod registry;
mod supervisor;

pub(crate) use image::locate_firmware_image;

pub use address::PhysAddr;
pub use handoff::HandoffBuffer;
pub use mmio::{MmioRegion, MmioValue};
pub use region::{DeviceRegisterRange, PhysAddrRange};
pub use registry::MemoryRegistry;
pub use supervisor::SupervisorMemory;
