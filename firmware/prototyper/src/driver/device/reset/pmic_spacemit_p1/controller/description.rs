//! Resources for the I2C controller used by the P1 PMIC.

use alloc::boxed::Box;
use runtime::{
    Result,
    memory::{MemoryRegistry, RegRange},
};

use crate::driver::Description;

use super::device::K1I2cController;

/// The K1 I2C controller registers and timebase needed to reach the PMIC.
#[derive(Clone, Copy)]
pub(in super::super) struct Desc {
    pub(in super::super) registers: RegRange,
    pub(in super::super) timebase_frequency_hz: Option<u32>,
}

impl Description for Desc {
    type Device = K1I2cController;

    fn init(self: Box<Self>, memory: &mut MemoryRegistry) -> Result<Self::Device> {
        K1I2cController::init(*self, memory)
    }
}
