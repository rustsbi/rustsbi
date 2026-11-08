//! V821 USB DMA bypass description.

use alloc::boxed::Box;
use runtime::{
    Result,
    memory::{MemoryRegistry, RegRange},
};

use super::device::UsbDmaBypass;
use crate::driver::Description;

pub(crate) struct Desc {
    pub(crate) registers: RegRange,
}

impl Description for Desc {
    type Device = UsbDmaBypass;

    fn init(self: Box<Self>, memory: &mut MemoryRegistry) -> Result<Self::Device> {
        UsbDmaBypass::init(*self, memory)
    }
}
