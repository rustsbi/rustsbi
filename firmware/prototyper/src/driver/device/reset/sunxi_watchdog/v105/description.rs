//! Description of the Sunxi V105 watchdog and its optional RTC gate.

use alloc::boxed::Box;
use runtime::{
    Result,
    memory::{MemoryRegistry, RegRange},
};

use crate::driver::Description;

use super::{super::super::ResetDevice, device::V105Watchdog};

/// Validated watchdog registers and an optional reset gate.
///
/// Neither MMIO region has been acquired yet.
pub(in super::super::super) struct Desc {
    pub(super) registers: RegRange,
    pub(super) reset_gate: Option<RegRange>,
}

impl Description for Desc {
    type Device = Box<dyn ResetDevice>;

    fn init(self: Box<Self>, memory: &mut MemoryRegistry) -> Result<Self::Device> {
        Ok(Box::new(V105Watchdog::init(*self, memory)?))
    }
}
