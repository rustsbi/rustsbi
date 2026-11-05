//! Syscon poweroff and reboot driver.
//!
//! Each action uses a syscon provider and applies a masked register update.
//!
//! # References
//!
//! - Devicetree binding: [syscon poweroff](https://github.com/torvalds/linux/blob/a500db7819c50db59e55f1b4fa1c3baa5a2616f3/Documentation/devicetree/bindings/power/reset/syscon-poweroff.yaml)
//!   — poweroff properties and masked register update.
//! - Devicetree binding: [syscon reboot](https://github.com/torvalds/linux/blob/a500db7819c50db59e55f1b4fa1c3baa5a2616f3/Documentation/devicetree/bindings/power/reset/syscon-reboot.yaml)
//!   — reboot properties and priority selection.

mod description;
mod device;

use runtime::{PlatformView, Result};

use crate::devicetree::try_for_each_enabled_node;

use description::ActionDesc;
pub(super) use description::Desc;

const POWEROFF_COMPATIBLE: &str = "syscon-poweroff";
const REBOOT_COMPATIBLE: &str = "syscon-reboot";

/// Discovers the highest-priority poweroff and reboot actions.
pub(super) fn describe(platform: &PlatformView<'_>) -> Result<Option<Desc>> {
    let mut poweroff = None;
    let mut reboot = None;
    try_for_each_enabled_node(platform.root(), &mut |discovered, _| {
        let Some(compatibles) = discovered.compatible() else {
            return Ok(());
        };
        let node = discovered.node();
        let parent = discovered.parent();
        if compatibles
            .all()
            .any(|compatible| compatible == POWEROFF_COMPATIBLE)
        {
            consider(
                &mut poweroff,
                ActionDesc::from_node(platform, node, parent)?,
            );
        }
        if compatibles
            .all()
            .any(|compatible| compatible == REBOOT_COMPATIBLE)
        {
            consider(&mut reboot, ActionDesc::from_node(platform, node, parent)?);
        }
        Ok(())
    })?;
    Ok((poweroff.is_some() || reboot.is_some()).then_some(Desc { poweroff, reboot }))
}

fn consider(selected: &mut Option<ActionDesc>, candidate: ActionDesc) {
    // Prefer the highest priority for this action; keep the first tie.
    if selected.is_none_or(|current| candidate.priority > current.priority) {
        *selected = Some(candidate);
    }
}
