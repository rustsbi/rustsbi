//! Devicetree description of NS16550-compatible UARTs.

use alloc::boxed::Box;
use runtime::{
    Result,
    console::ConsoleDevice,
    memory::{MemoryRegistry, RegRange},
};

use crate::driver::Description;

use super::device::{Device, U8RegisterAccess, U32RegisterAccess};

/// Parsed resources of one byte-spaced 16550 UART.
pub(super) struct DescU8 {
    pub(super) registers: RegRange,
}

/// Parsed resources of one word-spaced 16550 UART.
pub(super) struct DescU32 {
    pub(super) registers: RegRange,
}

impl Description for DescU8 {
    type Device = Box<dyn ConsoleDevice>;

    fn init(self: Box<Self>, memory: &mut MemoryRegistry) -> Result<Self::Device> {
        Ok(Box::new(Device::<U8RegisterAccess>::init(*self, memory)?))
    }
}

impl Description for DescU32 {
    type Device = Box<dyn ConsoleDevice>;

    fn init(self: Box<Self>, memory: &mut MemoryRegistry) -> Result<Self::Device> {
        Ok(Box::new(Device::<U32RegisterAccess>::init(*self, memory)?))
    }
}
