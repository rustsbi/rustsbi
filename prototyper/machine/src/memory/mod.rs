//! Physical-memory capabilities owned by the machine layer.

mod io;
mod supervisor;

pub use io::{IoMem, IoMemError, IoMemRegion, IoValue, io_fence};
pub(crate) use io::{claimed_ranges, initialize, reserve_ranges, seal};
pub use supervisor::{MemoryError, Reader, SupervisorMemory, Writer};
