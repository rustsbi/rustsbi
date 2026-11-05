//! Description of the SpacemiT P1 PMIC reset.

use alloc::boxed::Box;
use runtime::{Result, memory::MemoryRegistry};

use crate::driver::Description;

use super::{super::ResetDevice, controller, device::P1Pmic};

/// A validated 7-bit I2C device address.
#[derive(Clone, Copy)]
pub(super) struct I2cAddress(u8);

impl I2cAddress {
    pub(super) fn new(address: usize) -> Option<Self> {
        u8::try_from(address)
            .ok()
            .filter(|address| *address < 1 << 7)
            .map(Self)
    }

    pub(super) const fn get(self) -> u8 {
        self.0
    }
}

/// A validated PMIC address and the registers of its parent I2C controller.
///
/// The controller's MMIO region has not been acquired yet.
#[derive(Clone, Copy)]
pub(in super::super) struct Desc {
    pub(super) controller: controller::Desc,
    pub(super) address: I2cAddress,
}

impl Description for Desc {
    type Device = Box<dyn ResetDevice>;

    fn init(self: Box<Self>, memory: &mut MemoryRegistry) -> Result<Self::Device> {
        Ok(Box::new(P1Pmic::init(*self, memory)?))
    }
}
