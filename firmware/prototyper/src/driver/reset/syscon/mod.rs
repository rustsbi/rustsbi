//! Syscon poweroff and reboot driver.
//!
//! This module is the reset-driver adapter. Devicetree parsing stays in
//! [`description`], while acquired MMIO and command execution stay in
//! [`device`].
//!
//! # References
//!
//! - Devicetree binding: [syscon poweroff](https://github.com/torvalds/linux/blob/a500db7819c50db59e55f1b4fa1c3baa5a2616f3/Documentation/devicetree/bindings/power/reset/syscon-poweroff.yaml)
//!   — poweroff properties and masked register update.
//! - Devicetree binding: [syscon reboot](https://github.com/torvalds/linux/blob/a500db7819c50db59e55f1b4fa1c3baa5a2616f3/Documentation/devicetree/bindings/power/reset/syscon-reboot.yaml)
//!   — reboot properties and priority selection.

mod description;
mod device;

use runtime::Result;
use serde_device_tree::buildin::Node;

use super::ResetBackend;
use super::registry::{BindResources, ResetDriver};
use description::ActionDescription;
use device::SysconReset;

const POWEROFF_COMPATIBLE: &str = "syscon-poweroff";
const REBOOT_COMPATIBLE: &str = "syscon-reboot";

/// Matches syscon reset nodes and retains their unbound descriptions.
#[derive(Default)]
pub(in crate::driver::reset) struct SysconDriver {
    poweroff: Option<ActionDescription>,
    reboot: Option<ActionDescription>,
}

impl SysconDriver {
    fn consider(selected: &mut Option<ActionDescription>, candidate: ActionDescription) {
        // Prefer the highest priority for this action; keep the first tie.
        if selected.is_none_or(|current| candidate.priority() > current.priority()) {
            *selected = Some(candidate);
        }
    }
}

impl ResetDriver for SysconDriver {
    fn probe(
        &mut self,
        platform: &runtime::PlatformView<'_>,
        node: &Node<'_>,
        parent: Option<&Node<'_>>,
    ) -> Result<()> {
        let Some(compatibles) = crate::devicetree::compatible_strings(node) else {
            return Ok(());
        };
        if compatibles
            .iter()
            .any(|compatible| compatible == POWEROFF_COMPATIBLE)
        {
            Self::consider(
                &mut self.poweroff,
                ActionDescription::from_node(platform, node, parent)?,
            );
        }
        if compatibles
            .iter()
            .any(|compatible| compatible == REBOOT_COMPATIBLE)
        {
            Self::consider(
                &mut self.reboot,
                ActionDescription::from_node(platform, node, parent)?,
            );
        }
        Ok(())
    }

    fn has_device(&self) -> bool {
        self.poweroff.is_some() || self.reboot.is_some()
    }

    fn bind(
        &self,
        resources: &mut BindResources<'_>,
    ) -> Result<alloc::boxed::Box<dyn ResetBackend>> {
        Ok(alloc::boxed::Box::new(SysconReset::bind(
            self.poweroff,
            self.reboot,
            resources.memory(),
        )?))
    }

    fn log_summary(&self) {
        info!(
            "{:<30}: Available (syscon: poweroff={}, reboot={})",
            "Platform Reset Extension",
            self.poweroff.is_some(),
            self.reboot.is_some(),
        );
    }
}
