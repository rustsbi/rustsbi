//! V821 platform description.

use alloc::boxed::Box;
use runtime::{Result, memory::MemoryRegistry, soc::allwinner::v821::AllwinnerV821Soc};

use crate::driver::Description;

use super::device::V821;

/// V821 platform resources collected from the device tree before initialization.
pub(crate) struct Desc {
    pub(super) soc: AllwinnerV821Soc,
}

impl Desc {
    pub(super) const fn new(soc: AllwinnerV821Soc) -> Self {
        Self { soc }
    }
}

impl Description for Desc {
    type Device = V821;

    fn init(self: Box<Self>, memory: &mut MemoryRegistry) -> Result<Self::Device> {
        V821::init(*self, memory)
    }
}
