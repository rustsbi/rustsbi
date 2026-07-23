//! Physical-memory capabilities owned by the machine layer.

mod io;
mod supervisor;

pub use io::{IoMem, IoMemError, IoMemRegion, IoValue};
pub use supervisor::{MemoryError, Reader, SupervisorMemory, Writer};
