//! T-Head CLINT timer description.

use alloc::boxed::Box;
use runtime::{
    Result,
    memory::{MemoryRegistry, RegRange},
    timer::TimerDevice,
};

use super::super::super::Description;
use super::device::THeadTimer;

/// A T-Head CLINT timer register window before acquisition.
pub(crate) struct Desc {
    pub(crate) registers: RegRange,
}

impl Description for Desc {
    type Device = Box<dyn TimerDevice>;

    fn init(self: Box<Self>, memory: &mut MemoryRegistry) -> Result<Self::Device> {
        Ok(Box::new(THeadTimer::init(*self, memory)?))
    }
}
