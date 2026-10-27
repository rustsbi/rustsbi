//! Platform facts and SBI PMU discovery from the device tree.

mod console;
mod harts;
mod imsic;
mod interrupts;

pub(crate) use console::stdout_node;

use crate::devicetree::try_for_each_enabled_node;
use crate::info::PlatformInfo;
use crate::sbi::pmu::SbiPmu;
use runtime::{PlatformView, Result};

/// Reads the platform facts and PMU mappings consumed by driver and SBI initialization.
pub(super) fn discover_platform(
    board: &mut PlatformInfo,
    platform: &PlatformView<'_>,
) -> Result<Option<SbiPmu>> {
    let cpu_interrupt_controllers = harts::discover(board, platform)?;
    let mut pmu_node = None;
    try_for_each_enabled_node(platform.root(), &mut |node, path| {
        if pmu_node.is_none()
            && node
                .compatible()
                .is_some_and(|values| values.all().any(|value| value == "riscv,pmu"))
        {
            pmu_node = Some(node.node());
        }
        interrupts::discover_node(board, platform, node, path, &cpu_interrupt_controllers)
    })?;
    match pmu_node {
        Some(node) => SbiPmu::from_node(node).map(Some),
        None => Ok(None),
    }
}
