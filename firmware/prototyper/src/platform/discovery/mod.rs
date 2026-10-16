//! Platform facts and SBI PMU discovery from the device tree.

mod console;
mod harts;
mod imsic;
mod interrupts;

use super::info::{BoardInfo, SocDescription};
use crate::devicetree::try_for_each_enabled_node;
use crate::sbi::pmu::SbiPmu;
use runtime::{
    PlatformView, Result,
    soc::allwinner::{v821::AllwinnerV821Soc, v861::AllwinnerV861Soc},
};

/// Reads the platform facts and PMU mappings consumed by driver and SBI initialization.
pub(super) fn discover_platform(
    board: &mut BoardInfo,
    platform: &PlatformView<'_>,
) -> Result<Option<SbiPmu>> {
    let cpu_interrupt_controllers = harts::discover(board, platform)?;
    board.devices.console = console::discover(platform)?;
    let mut reset = crate::driver::ResetDescription::new();
    let mut soc = if let Some(v821) = platform.soc::<AllwinnerV821Soc>()? {
        Some(SocDescription::V821(
            crate::platform::allwinner::v821::Description::new(v821),
        ))
    } else if let Some(k1) = platform.spacemit_k1_registers()? {
        Some(SocDescription::SpacemitK1(k1))
    } else {
        platform
            .soc::<AllwinnerV861Soc>()?
            .map(SocDescription::V861)
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
        reset.probe(platform, node)?;
        if let Some(SocDescription::V821(v821)) = &mut soc {
            v821.probe(platform, node)?;
        }
        interrupts::discover_node(board, platform, node, path, &cpu_interrupt_controllers)
    })?;
    board.devices.reset = reset;
    board.soc = soc;
    match pmu_node {
        Some(node) => SbiPmu::from_node(node).map(Some),
        None if matches!(&board.soc, Some(SocDescription::V861(_))) => SbiPmu::new().map(Some),
        None => Ok(None),
    }
}
