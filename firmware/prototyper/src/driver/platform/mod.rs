#![deny(unsafe_code)]

//! Platform discovery, initialization, and shared services.
//!
//! The boot hart reads the device tree and publishes the initialized services.

pub(crate) mod allwinner;
mod boot;
mod handoff;
mod harts;
mod interrupts;
mod report;
pub(crate) mod spacemit_k1;
mod wakeup;

pub(crate) mod qemu_aplic;

use crate::{
    devicetree::try_for_each_enabled_node,
    sbi::pmu::{self, SbiPmu},
};
use runtime::{
    PlatformView, Result,
    hart::HartId,
    soc::allwinner::{v821::AllwinnerV821Soc, v861::AllwinnerV861Soc},
};

pub use boot::{init_platform, wait_until_ready};

/// Selects SBI PMU information from the device tree or platform defaults.
fn describe_pmu(platform: &PlatformView<'_>) -> Result<Option<SbiPmu>> {
    let mut pmu_node = None;
    try_for_each_enabled_node(platform.root(), &mut |node, _| {
        if pmu_node.is_none()
            && node
                .compatible()
                .is_some_and(|values| values.all().any(|value| value == "riscv,pmu"))
        {
            pmu_node = Some(node.node());
        }
        Ok(())
    })?;
    if let Some(pmu) = pmu_node.and_then(pmu::from_node) {
        return Ok(Some(pmu));
    }
    if platform.soc::<AllwinnerV821Soc>()?.is_some() || platform.spacemit_k1_registers()?.is_some()
    {
        return Ok(None);
    }
    Ok(platform
        .soc::<AllwinnerV861Soc>()?
        .map(|_| SbiPmu::default()))
}

/// Performs platform-specific setup on the current secondary hart.
pub(crate) fn initialize_secondary_hart(hart: HartId) {
    spacemit_k1::initialize_secondary_hart(hart);
}
