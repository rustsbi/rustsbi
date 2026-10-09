//! Access to physical memory described by the boot device tree.
//!
//! [`crate::PlatformDescription::memory_resources`] returns supervisor RAM with reserved
//! memory and the firmware image excluded, plus a [`MemoryRegistry`] from
//! which non-overlapping device register windows can be acquired.
//! [`crate::boot::embedded_payload`] and [`crate::boot::embedded_fdt`] return
//! linked handoff ranges without retaining Rust references to their contents.

mod address;
mod image;
mod mmio;
mod region;
mod registry;
mod supervisor;

pub(crate) use image::linker_image_bounds;
pub use image::{FirmwareImageLayout, firmware_image_layout};

pub use address::PhysAddr;
pub use mmio::{MmioRegion, MmioValue};
pub use region::{DeviceRegisterRange, PhysAddrRange};
pub use registry::MemoryRegistry;
pub use supervisor::SupervisorMemory;
