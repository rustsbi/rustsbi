//! Platform facts and SBI PMU discovery from the device tree.

mod console;
mod harts;
mod imsic;
mod interrupts;

use super::info::{BoardInfo, SocDescription};
use crate::devicetree::try_for_each_enabled_node;
use crate::sbi::pmu::{self, SbiPmu};

/// Reads the platform facts and PMU mappings consumed by driver and SBI initialization.
pub(super) fn discover_platform(
    platform: &runtime::PlatformView<'_>,
) -> runtime::Result<(BoardInfo, Option<SbiPmu>)> {
    let mut board = BoardInfo::empty();
    let cpu_interrupt_controllers = harts::discover(&mut board, platform)?;
    board.devices.console = console::discover(platform)?;
    let mut reset = crate::driver::ResetDescription::new();
    let mut soc =
        if let Some(v821) = platform.soc::<runtime::soc::allwinner::v821::AllwinnerV821Soc>()? {
            Some(SocDescription::V821(
                crate::platform::allwinner::v821::Description::new(v821),
            ))
        } else if let Some(k1) = platform.spacemit_k1_registers()? {
            Some(SocDescription::SpacemitK1(k1))
        } else {
            platform
                .soc::<runtime::soc::allwinner::v861::AllwinnerV861Soc>()?
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
        interrupts::discover_node(&mut board, platform, node, path, &cpu_interrupt_controllers)
    })?;
    board.devices.reset = reset;
    board.soc = soc;
    let pmu = pmu_node.and_then(pmu::from_node).or_else(|| {
        matches!(&board.soc, Some(SocDescription::V861(_))).then_some(SbiPmu::default())
    });
    Ok((board, pmu))
}
