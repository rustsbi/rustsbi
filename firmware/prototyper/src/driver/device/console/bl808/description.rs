//! Devicetree description of the Bouffalo Lab BL808 UART.

use alloc::boxed::Box;
use runtime::{
    Result,
    console::ConsoleDevice,
    memory::{MemoryRegistry, RegRange},
};

use crate::driver::Description;

use super::device::Device;

/// Parsed resources of one BL808 UART instance.
pub(super) struct Desc {
    pub(super) registers: RegRange,
}

impl Description for Desc {
    type Device = Box<dyn ConsoleDevice>;

    fn init(self: Box<Self>, memory: &mut MemoryRegistry) -> Result<Self::Device> {
        Ok(Box::new(Device::init(*self, memory)?))
    }
}
