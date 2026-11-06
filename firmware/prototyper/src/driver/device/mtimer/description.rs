//! Parsed machine timer resources.

use alloc::boxed::Box;
use runtime::{
    Result,
    memory::{MemoryRegistry, RegRange},
    timer::TimerDevice,
};

use super::super::super::Description;
use super::super::hart_slot::HartSlotMap;
use super::device::Mtimer;

/// How a machine timer exposes its 64-bit compare registers.
#[derive(Clone, Copy)]
pub(crate) enum CompareAccess {
    /// A 64-bit MMIO access on RV64 and two 32-bit words on RV32.
    Native,
    /// Two 32-bit MMIO words on every target.
    TwoWords,
}

/// A machine timer before register-window acquisition.
pub(crate) struct Desc {
    pub(crate) compare: RegRange,
    pub(crate) time: Option<RegRange>,
    pub(crate) hart_slots: HartSlotMap,
    pub(crate) access: CompareAccess,
    pub(crate) kind: &'static str,
}

impl Description for Desc {
    type Device = Box<dyn TimerDevice>;

    fn init(self: Box<Self>, memory: &mut MemoryRegistry) -> Result<Self::Device> {
        Ok(Box::new(Mtimer::init(*self, memory)?))
    }
}
