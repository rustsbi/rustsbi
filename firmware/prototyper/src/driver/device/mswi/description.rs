//! Parsed machine software-interrupt resources.

use alloc::boxed::Box;
use runtime::{
    Result,
    ipi::{IpiDevice, Software},
    memory::{MemoryRegistry, RegRange},
};

use super::super::super::Description;
use super::super::hart_slot::HartSlotMap;
use super::device::Mswi;

/// A machine software-interrupt register window before acquisition.
pub(crate) struct Desc {
    pub(crate) registers: RegRange,
    pub(crate) hart_slots: HartSlotMap,
    pub(crate) kind: &'static str,
}

impl Description for Desc {
    type Device = Box<dyn IpiDevice<Software>>;

    fn init(self: Box<Self>, memory: &mut MemoryRegistry) -> Result<Self::Device> {
        Ok(Box::new(Mswi::init(*self, memory)?))
    }
}
