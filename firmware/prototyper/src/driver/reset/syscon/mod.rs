//! Syscon poweroff and reboot driver.
//!
//! Devicetree parsing stays in [`description`], while acquired MMIO and
//! command execution stay in [`device`].
//!
//! # References
//!
//! - Devicetree binding: [syscon poweroff](https://github.com/torvalds/linux/blob/a500db7819c50db59e55f1b4fa1c3baa5a2616f3/Documentation/devicetree/bindings/power/reset/syscon-poweroff.yaml)
//!   — poweroff properties and masked register update.
//! - Devicetree binding: [syscon reboot](https://github.com/torvalds/linux/blob/a500db7819c50db59e55f1b4fa1c3baa5a2616f3/Documentation/devicetree/bindings/power/reset/syscon-reboot.yaml)
//!   — reboot properties and priority selection.

mod description;
mod device;

use alloc::boxed::Box;
use runtime::{PlatformView, Result};

use crate::devicetree::{EnabledNode, try_for_each_enabled_node};
use crate::driver::device::Driver;

use super::{Description, Reset, ResetDevice};
use description::ActionDescription;
use device::SysconReset;

const POWEROFF_COMPATIBLE: &str = "syscon-poweroff";
const REBOOT_COMPATIBLE: &str = "syscon-reboot";

pub(in crate::driver) struct SysconDriver;

pub(super) static SYSCON: SysconDriver = SysconDriver;

impl Driver<Reset> for SysconDriver {
    fn compatibles(&self) -> &'static [&'static str] {
        &[POWEROFF_COMPATIBLE, REBOOT_COMPATIBLE]
    }

    fn parse<'a, 'tree>(
        &self,
        platform: &'a PlatformView<'tree>,
        _device: EnabledNode<'a, 'tree>,
        _compatible: &str,
    ) -> Result<Option<Description>> {
        // The class selects one backend, so collect both independent actions
        // before binding that backend. Parsing does not acquire MMIO.
        let mut poweroff = None;
        let mut reboot = None;
        try_for_each_enabled_node(platform.root(), &mut |device, _| {
            let Some(compatibles) = device.compatible() else {
                return Ok(());
            };
            for compatible in compatibles.all() {
                let selected = match compatible {
                    POWEROFF_COMPATIBLE => &mut poweroff,
                    REBOOT_COMPATIBLE => &mut reboot,
                    _ => continue,
                };
                let candidate =
                    ActionDescription::from_node(platform, device.node(), device.parent())?;
                // Prefer the highest priority for each action; keep the first tie.
                if selected.is_none_or(|current: ActionDescription| {
                    candidate.priority() > current.priority()
                }) {
                    *selected = Some(candidate);
                }
            }
            Ok(())
        })?;
        let Some(action) = poweroff.or(reboot) else {
            return Ok(None);
        };
        Ok(Some(Description::new(
            "syscon",
            action.registers(),
            move |memory| {
                Ok(Box::new(SysconReset::bind(poweroff, reboot, memory)?) as Box<dyn ResetDevice>)
            },
        )))
    }
}
