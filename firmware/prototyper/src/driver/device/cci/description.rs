//! CCI-550 register windows selected by the platform.

use alloc::boxed::Box;
use runtime::{
    Result,
    memory::{MemoryRegistry, RegRange},
};

use super::super::Description;
use super::Cci550;

/// Register windows of one platform-selected CCI-550 instance.
pub(crate) struct Desc<const N: usize> {
    /// Status-register window.
    pub(crate) status: RegRange,
    /// Snoop-control windows of the selected slave interfaces.
    pub(crate) snoop_controls: [RegRange; N],
}

impl<const N: usize> Description for Desc<N> {
    type Device = Cci550<N>;

    fn init(self: Box<Self>, memory: &mut MemoryRegistry) -> Result<Self::Device> {
        Cci550::init(*self, memory)
    }
}
