//! Description of the SiFive test finisher.

use alloc::boxed::Box;
use runtime::{
    Result,
    memory::{MemoryRegistry, RegRange},
};

use crate::driver::Description;

use super::{super::ResetDevice, device::SifiveTestDevice};

/// Validated reset registers whose MMIO region has not been acquired yet.
pub(in super::super) struct Desc {
    pub(super) registers: RegRange,
}

impl Description for Desc {
    type Device = Box<dyn ResetDevice>;

    fn init(self: Box<Self>, memory: &mut MemoryRegistry) -> Result<Self::Device> {
        Ok(Box::new(SifiveTestDevice::init(*self, memory)?))
    }
}
