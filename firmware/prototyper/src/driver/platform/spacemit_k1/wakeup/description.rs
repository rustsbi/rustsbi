//! K1 PMU secondary-hart wakeup description.

use alloc::boxed::Box;
use runtime::{
    Result, SpacemitK1Registers,
    memory::{MemoryRegistry, RegRange},
};

use super::device::K1Wakeup;
use crate::driver::Description;

pub(crate) struct Desc {
    pub(super) status: RegRange,
    pub(super) controls: [RegRange; 2],
}

impl Desc {
    pub(crate) fn new(registers: SpacemitK1Registers) -> Self {
        Self {
            status: registers.core_status(),
            controls: registers.wakeup_controls(),
        }
    }
}

impl Description for Desc {
    type Device = K1Wakeup;

    fn init(self: Box<Self>, memory: &mut MemoryRegistry) -> Result<Self::Device> {
        K1Wakeup::init(*self, memory)
    }
}
