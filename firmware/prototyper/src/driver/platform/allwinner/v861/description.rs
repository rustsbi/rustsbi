//! V861 C907 hart-release description.

use alloc::boxed::Box;
use runtime::{
    Result, hart::HartWake, memory::MemoryRegistry, soc::allwinner::v861::AllwinnerV861Soc,
};

use super::device::V861HartRelease;
use crate::driver::Description;

pub(crate) struct Desc {
    pub(crate) soc: AllwinnerV861Soc,
}

impl Description for Desc {
    type Device = Box<dyn HartWake>;

    fn init(self: Box<Self>, memory: &mut MemoryRegistry) -> Result<Self::Device> {
        Ok(Box::new(V861HartRelease::init(*self, memory)?))
    }
}
