//! Devicetree discovery for reset drivers.
//!
//! [`Description`] owns the reset-local registry during discovery, then binds
//! only the highest-priority matched driver into a [`ResetDevice`].

use runtime::Result;

use crate::devicetree::EnabledNode;

use super::ResetDevice;
use super::registry::{BindResources, Registry};

/// Reset-driver descriptions collected from one Platform Description view.
pub(crate) struct Description {
    registry: Registry,
}

impl Description {
    /// Returns an empty placeholder used while constructing [`BoardInfo`](crate::platform::BoardInfo).
    pub(crate) const fn empty() -> Self {
        Self {
            registry: Registry::empty(),
        }
    }

    /// Creates a reset description ready for the shared discovery pass.
    pub(crate) fn new() -> Self {
        Self {
            registry: Registry::new(),
        }
    }

    /// Lets every registered reset driver inspect one enabled node.
    pub(crate) fn probe(
        &mut self,
        platform: &runtime::PlatformView<'_>,
        node: EnabledNode<'_, '_>,
    ) -> Result<()> {
        self.registry.probe(platform, node)
    }

    /// Binds the first discovered driver in registration priority order.
    pub(crate) fn bind(
        &self,
        timebase_frequency_hz: Option<u32>,
        memory: &mut runtime::memory::MemoryRegistry,
    ) -> Result<ResetDevice> {
        let mut resources = BindResources::new(memory, timebase_frequency_hz);
        Ok(ResetDevice::new(self.registry.bind_first(&mut resources)?))
    }

    /// Logs the highest-priority discovered reset description.
    pub(crate) fn log_summary(&self) {
        if !self.registry.log_selected() {
            warn!("{:<30}: Not Available", "Platform Reset Device");
        }
    }
}
