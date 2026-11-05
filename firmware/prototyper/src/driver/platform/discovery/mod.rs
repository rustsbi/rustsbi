//! Platform facts and SBI PMU discovery from the device tree.

mod harts;
mod imsic;
mod interrupts;

use super::info::{BoardInfo, SocDescription};
use crate::devicetree::try_for_each_enabled_node;
use crate::sbi::pmu::{self, SbiPmu};
use runtime::{PlatformView, Result};

/// Reads the platform facts and PMU mappings consumed by driver and SBI initialization.
pub(super) fn discover_platform(
    board: &mut BoardInfo,
    platform: &PlatformView<'_>,
) -> Result<Option<SbiPmu>> {
    let cpu_interrupt_controllers = harts::discover(board, platform)?;
    let soc = if let Some(v821) = crate::platform::allwinner::v821::describe(platform)? {
        Some(SocDescription::V821(v821))
    } else if let Some(k1) = crate::platform::spacemit_k1::describe(platform)? {
        Some(SocDescription::SpacemitK1(k1.registers))
    } else {
        crate::platform::allwinner::v861::describe(platform)?
            .map(|description| SocDescription::V861(description.soc))
    };
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
    board.soc = soc;
    let pmu = pmu_node.and_then(pmu::from_node).or_else(|| {
        matches!(&board.soc, Some(SocDescription::V861(_))).then_some(SbiPmu::default())
    });
    Ok(pmu)
}
