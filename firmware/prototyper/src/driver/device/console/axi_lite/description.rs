//! Devicetree description of the Xilinx AXI UART Lite.

use alloc::boxed::Box;
use runtime::{
    Result,
    console::ConsoleDevice,
    memory::{MemoryRegistry, RegRange},
};

use crate::driver::Description;

use super::device::Device;

/// Parsed resources of one AXI UART Lite instance.
pub(super) struct Desc {
    pub(super) registers: RegRange,
}

impl Description for Desc {
    type Device = Box<dyn ConsoleDevice>;

    fn init(self: Box<Self>, memory: &mut MemoryRegistry) -> Result<Self::Device> {
        Ok(Box::new(Device::init(*self, memory)?))
    }
}
