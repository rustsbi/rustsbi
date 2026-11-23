//! System-reset firmware drivers.
//!
//! Discovery selects one supported reset description without acquiring MMIO.
//! Initialization then creates the single [`ResetDevice`] managed by `runtime`.

mod pmic_spacemit_p1;
mod selection;
mod sifive_test;
mod sunxi_watchdog;
mod syscon;

use runtime::{
    Error, FdtNode, PlatformView, Result,
    memory::{MemoryRegistry, RegRange},
};

use pmic_spacemit_p1::P1Pmic;
use sifive_test::SifiveTestDevice;
use sunxi_watchdog::{v104::V104Watchdog, v105::V105Watchdog};
use syscon::SysconReset;

pub(crate) use runtime::reset::{ResetDevice, ResetReason, ResetRequest, ResetType};
pub(crate) use selection::{Desc, describe};

/// Initializes the selected device and transfers ownership to Runtime.
pub(crate) fn init(description: Desc, memory: &mut MemoryRegistry) -> Result<()> {
    match description {
        Desc::SifiveTest(desc) => runtime::reset::init(SifiveTestDevice::init(desc, memory)?),
        Desc::SpacemitP1(desc) => runtime::reset::init(P1Pmic::init(desc, memory)?),
        Desc::SunxiV105(desc) => runtime::reset::init(V105Watchdog::init(desc, memory)?),
        Desc::SunxiV104(desc) => runtime::reset::init(V104Watchdog::init(desc, memory)?),
        Desc::Syscon(desc) => runtime::reset::init(SysconReset::init(desc, memory)?),
    }
}

fn primary_registers(platform: &PlatformView<'_>, node: FdtNode<'_, '_>) -> Result<RegRange> {
    platform.device_register(node)?.ok_or(Error::InvalidArgs)
}
