//! Description of the Sunxi V104 watchdog.

use alloc::boxed::Box;
use runtime::{
    Result,
    memory::{MemoryRegistry, RegRange},
};

use crate::driver::Description;

use super::{super::super::ResetDevice, device::V104Watchdog};

/// Validated watchdog registers whose MMIO region has not been acquired yet.
pub(in super::super::super) struct Desc {
    pub(super) registers: RegRange,
    pub(super) timebase_frequency_hz: Option<u32>,
}

impl Description for Desc {
    type Device = Box<dyn ResetDevice>;

    fn init(self: Box<Self>, memory: &mut MemoryRegistry) -> Result<Self::Device> {
        Ok(Box::new(V104Watchdog::init(*self, memory)?))
    }
}
