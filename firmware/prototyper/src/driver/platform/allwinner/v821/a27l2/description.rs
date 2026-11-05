//! V821 A27L2 cache description.

use alloc::boxed::Box;
use runtime::{
    Result,
    memory::{MemoryRegistry, RegRange},
    soc::allwinner::v821::AllwinnerV821Soc,
};

use super::device::A27L2Cache;
use crate::driver::Description;

pub(crate) struct Desc {
    pub(crate) soc: AllwinnerV821Soc,
    pub(crate) registers: RegRange,
}

impl Description for Desc {
    type Device = A27L2Cache;

    #[inline]
    fn init(self: Box<Self>, memory: &mut MemoryRegistry) -> Result<Self::Device> {
        A27L2Cache::init(*self, memory)
    }
}
