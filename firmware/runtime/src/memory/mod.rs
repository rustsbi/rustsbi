//! Access to physical memory described by the boot device tree.
//!
//! On firmware targets, [`crate::PlatformDescription`] provides supervisor RAM
//! with reserved memory and the firmware image excluded, plus a [`MemoryRegistry`]
//! for acquiring non-overlapping device register windows.
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
