//! Device-tree descriptions of the timer and interrupt devices.

use runtime::{Error, FdtNode, PlatformView, Result, memory::RegRange};

use super::{clint, imsic, match_desc, plicsw, plmt, thead_plic};
use crate::devicetree::{EnabledNode, NodeSelection, try_for_each_enabled_node};

/// Candidates retained until the platform selects and initializes its devices.
#[derive(Default)]
pub(crate) struct Selection {
    pub(crate) clint: Option<NodeSelection<clint::Desc>>,
    pub(crate) imsic: Option<NodeSelection<imsic::Desc>>,
    pub(crate) machine_aplic: Option<NodeSelection<RegRange>>,
    pub(crate) machine_aplic_delegated: bool,
    pub(crate) thead_plic: Option<RegRange>,
    pub(crate) plmt: Option<plmt::Desc>,
    pub(crate) plicsw: Option<plicsw::Desc>,
}

/// Parses enabled nodes in device-tree order before initializing any device.
pub(crate) fn describe(platform: &PlatformView<'_>) -> Result<Selection> {
    let mut selected = Selection::default();
    try_for_each_enabled_node(platform.root(), &mut |node, path| {
        describe_node(&mut selected, platform, node, path)
    })?;
    Ok(selected)
}

/// Matches one node's compatible strings in their declared order.
fn describe_node(
    selected: &mut Selection,
    platform: &PlatformView<'_>,
    discovered: EnabledNode<'_, '_>,
    source_path: &[&str],
) -> Result<()> {
    let Some(compatibles) = discovered.compatible() else {
        return Ok(());
    };
    let node = discovered.node();
    match_desc!(first compatible in compatibles.all(), source_path; {
        plmt::COMPATIBLE => once(selected.plmt, plmt::describe(platform, node)),
        plicsw::COMPATIBLE => once(selected.plicsw, plicsw::describe(platform, node)),
        compatible if clint::supports(compatible) => node(
            selected.clint, clint::describe(platform, node)
        ),
        compatible if imsic::IMSIC_COMPATIBLES.contains(&compatible) => optional_node(
            selected.imsic, imsic::describe(platform, node)
        ),
        compatible if is_machine_aplic(node, compatible) => node(
            selected.machine_aplic,
            platform.device_register(node)?.ok_or(Error::InvalidArgs),
            {
                selected.machine_aplic_delegated =
                    node.property("riscv,delegate").is_some()
                        || node.property("riscv,delegation").is_some();
            }
        ),
        compatible if thead_plic::COMPATIBLES.contains(&compatible) => once(
            selected.thead_plic,
            platform.device_register(node)?.ok_or(Error::InvalidArgs)
        )
    })
}

fn is_machine_aplic(node: FdtNode<'_, '_>, compatible: &str) -> bool {
    compatible == "riscv,aplic" && node.property("riscv,children").is_some()
}
