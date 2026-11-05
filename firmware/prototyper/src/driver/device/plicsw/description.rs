//! Devicetree description of an Allwinner PLICSW.

use alloc::boxed::Box;
use runtime::{
    Result,
    ipi::{IpiDevice, Software},
    memory::{MemoryRegistry, RegRange},
};

use super::super::Description;
use super::device::PlicSw;

/// A PLICSW register window before acquisition.
pub(crate) struct Desc {
    pub(crate) registers: RegRange,
}

impl Description for Desc {
    type Device = Box<dyn IpiDevice<Software>>;

    fn init(self: Box<Self>, memory: &mut MemoryRegistry) -> Result<Self::Device> {
        Ok(Box::new(PlicSw::init(*self, memory)?))
    }
}
