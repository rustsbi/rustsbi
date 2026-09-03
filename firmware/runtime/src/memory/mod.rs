//! Validated access to physical memory.
//!
//! [`MemoryRegistry`] records the platform's RAM, reserved, and MMIO ranges.
//! It returns bounded [`SupervisorMemory`] and [`MmioRegion`] handles without
//! exposing raw memory references.

mod address;
mod mmio;
mod region;
mod registry;
mod supervisor;

pub use address::PhysAddr;
pub use mmio::{MmioRegion, MmioValue};
pub use region::PhysAddrRange;
pub use registry::MemoryRegistry;
pub use supervisor::{ReadMemory, SupervisorMemory, WriteMemory};
