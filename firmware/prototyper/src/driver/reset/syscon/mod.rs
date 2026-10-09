//! Syscon poweroff and reboot driver.
//!
//! Poweroff and reboot descriptions select independent masked register
//! updates. The selected reset controller serializes their execution.
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
use runtime::memory::MemoryRegistry;

use crate::devicetree::EnabledNode;

use super::ResetDevice;
use super::registry::ResetDriver;
use description::ActionDescription;
use device::SysconReset;

const POWEROFF_COMPATIBLE: &str = "syscon-poweroff";
const REBOOT_COMPATIBLE: &str = "syscon-reboot";

/// Devicetree descriptions for matched syscon reset nodes.
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
        discovered: EnabledNode<'_, '_>,
    ) -> Result<()> {
        let Some(compatibles) = discovered.compatible() else {
            return Ok(());
        };
        let node = discovered.node();
        let parent = discovered.parent();
        if compatibles
            .all()
            .any(|compatible| compatible == POWEROFF_COMPATIBLE)
        {
            Self::consider(
                &mut self.poweroff,
                ActionDescription::from_node(platform, node, parent)?,
            );
        }
        if compatibles
            .all()
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
        memory: &mut MemoryRegistry,
        _timebase_frequency_hz: Option<u32>,
    ) -> Result<alloc::boxed::Box<dyn ResetDevice>> {
        Ok(alloc::boxed::Box::new(SysconReset::bind(
            self.poweroff,
            self.reboot,
            memory,
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
