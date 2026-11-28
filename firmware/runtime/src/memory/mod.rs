//! Access to physical memory described by the boot device tree.
//!
//! The boot hart takes the [`MemoryRegistry`] from [`crate::PlatformDescription`]
//! to bind non-overlapping device register windows.
//! Handoff writes the next stage's device tree before releasing [`SupervisorMemory`],
//! whose physical ranges exclude platform reservations and the complete firmware image.
//! [`HandoffBuffer`] holds linked bytes whose ownership passes to the next
//! stage without retaining a Rust reference to their contents.

mod address;
mod handoff;
mod mmio;
mod region;
mod registry;
mod supervisor;

pub use address::PhysAddr;
pub use handoff::HandoffBuffer;
pub use mmio::{MmioRegion, MmioValue};
pub use region::{PhysAddrRange, RegRange};
pub use registry::MemoryRegistry;
pub use supervisor::SupervisorMemory;
