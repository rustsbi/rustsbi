//! CLINT software-interrupt description.

use alloc::boxed::Box;
use runtime::{
    Result,
    ipi::{IpiDevice, Software},
    memory::{MemoryRegistry, RegRange},
};

use super::super::super::Description;
use super::device::ClintIpi;

/// A CLINT software-interrupt register window before acquisition.
#[derive(Clone, Copy)]
pub(crate) struct Desc {
    pub(crate) registers: RegRange,
}

impl Description for Desc {
    type Device = Box<dyn IpiDevice<Software>>;

    fn init(self: Box<Self>, memory: &mut MemoryRegistry) -> Result<Self::Device> {
        Ok(Box::new(ClintIpi::init(*self, memory)?))
    }
}
