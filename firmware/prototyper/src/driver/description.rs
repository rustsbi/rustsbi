//! Initialization of a parsed device or platform description.

use alloc::boxed::Box;
use runtime::{Result, memory::MemoryRegistry};

/// A hardware description that can be initialized.
pub(crate) trait Description {
    /// The initialized object type.
    type Device;

    /// Initializes the described object and acquires any resources it needs.
    fn init(self: Box<Self>, memory: &mut MemoryRegistry) -> Result<Self::Device>;
}
