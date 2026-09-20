//! Devicetree discovery for reset drivers.
//!
//! [`Description`] owns the reset-local registry during discovery, then binds
//! only the highest-priority matched driver into a [`ResetDevice`].

use runtime::Result;
use serde_device_tree::buildin::Node;

use super::ResetDevice;
use super::registry::{BindResources, Registry};

/// Reset-driver descriptions collected from one Platform Description view.
pub(crate) struct Description {
    registry: Registry,
}

impl Description {
    /// An empty description used before the single discovery pass.
    pub(crate) fn empty() -> Self {
        Self {
            registry: Registry::empty(),
        }
    }

    /// Walks the tree once and lets every registered reset driver inspect it.
    pub(crate) fn discover(platform: &runtime::PlatformView<'_>) -> Result<Self> {
        let mut description = Self {
            registry: Registry::new(),
        };
        description.visit(platform, platform.root(), None)?;
        Ok(description)
    }

    fn visit<'tree>(
        &mut self,
        platform: &runtime::PlatformView<'tree>,
        node: &Node<'tree>,
        parent: Option<&Node<'tree>>,
    ) -> Result<()> {
        if !runtime::node_is_enabled(node) {
            return Ok(());
        }
        self.registry.probe(platform, node, parent)?;
        for child in node.nodes() {
            let child = child.deserialize::<Node<'tree>>();
            self.visit(platform, &child, Some(node))?;
        }
        Ok(())
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
