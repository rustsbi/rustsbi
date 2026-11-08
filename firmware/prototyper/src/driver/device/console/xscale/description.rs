//! Devicetree description of the Intel XScale/PXA UART.

use alloc::boxed::Box;
use runtime::{
    Result,
    console::ConsoleDevice,
    memory::{MemoryRegistry, RegRange},
};

use crate::driver::Description;

use super::device::Device;

/// Parsed resources of one XScale UART instance.
pub(super) struct Desc {
    pub(super) registers: RegRange,
    pub(super) clock_hz: Option<u32>,
}

impl Description for Desc {
    type Device = Box<dyn ConsoleDevice>;

    fn init(self: Box<Self>, memory: &mut MemoryRegistry) -> Result<Self::Device> {
        Ok(Box::new(Device::init(*self, memory)?))
    }
}
