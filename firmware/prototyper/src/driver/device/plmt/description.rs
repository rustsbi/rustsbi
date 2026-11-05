//! Devicetree description of an Andes PLMT.

use alloc::boxed::Box;
use runtime::{
    Result,
    memory::{MemoryRegistry, RegRange},
    timer::TimerDevice,
};

use super::super::Description;
use super::device::Plmt;

/// A PLMT register window before acquisition.
pub(crate) struct Desc {
    pub(crate) registers: RegRange,
}

impl Description for Desc {
    type Device = Box<dyn TimerDevice>;

    fn init(self: Box<Self>, memory: &mut MemoryRegistry) -> Result<Self::Device> {
        Ok(Box::new(Plmt::init(*self, memory)?))
    }
}
