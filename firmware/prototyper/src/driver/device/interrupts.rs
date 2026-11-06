//! Device-tree descriptions grouped by timer, IPI, and irqchip responsibility.

use runtime::{Error, FdtNode, PlatformView, Result, memory::RegRange};

use super::{aclint, clint, imsic, match_desc, mswi, mtimer, plicsw, plmt, thead_plic};
use crate::devicetree::{EnabledNode, NodeSelection, try_for_each_enabled_node};

/// Timer resources retained until platform initialization.
#[derive(Default)]
pub(crate) struct TimerSelection {
    pub(crate) aclint: Option<NodeSelection<mtimer::Desc>>,
    pub(crate) clint: Option<NodeSelection<mtimer::Desc>>,
    pub(crate) plmt: Option<plmt::Desc>,
}

/// IPI resources retained until platform initialization.
#[derive(Default)]
pub(crate) struct IpiSelection {
    pub(crate) aclint: Option<NodeSelection<mswi::Desc>>,
    pub(crate) clint: Option<NodeSelection<mswi::Desc>>,
    pub(crate) imsic: Option<NodeSelection<imsic::Desc>>,
    pub(crate) plicsw: Option<plicsw::Desc>,
}

/// Interrupt-controller resources independent of timer or IPI policy.
#[derive(Default)]
pub(crate) struct IrqchipSelection {
    pub(crate) machine_aplic: Option<NodeSelection<RegRange>>,
    pub(crate) machine_aplic_delegated: bool,
    pub(crate) thead_plic: Option<RegRange>,
}

/// Parsed resources grouped by their initialization responsibility.
pub(crate) struct Selection {
    pub(crate) timer: TimerSelection,
    pub(crate) ipi: IpiSelection,
    pub(crate) irqchip: IrqchipSelection,
}

#[derive(Default)]
struct Candidates {
    timer: TimerSelection,
    ipi: IpiSelection,
    irqchip: IrqchipSelection,
    clint: Option<NodeSelection<clint::Desc>>,
}

/// Parses enabled nodes in device-tree order before initializing any device.
pub(crate) fn describe(platform: &PlatformView<'_>) -> Result<Selection> {
    let mut selected = Candidates::default();
    try_for_each_enabled_node(platform.root(), &mut |node, path| {
        describe_node(&mut selected, platform, node, path)
    })?;
    let Candidates {
        mut timer,
        mut ipi,
        irqchip,
        clint,
    } = selected;
    if let Some(node) = clint {
        let (clint, path) = node.into_parts();
        timer.clint = Some(NodeSelection::new(clint.timer, path.clone()));
        ipi.clint = Some(NodeSelection::new(clint.ipi, path));
    }
    Ok(Selection {
        timer,
        ipi,
        irqchip,
    })
}

/// Matches one node's compatible strings in their declared order.
fn describe_node(
    selected: &mut Candidates,
    platform: &PlatformView<'_>,
    discovered: EnabledNode<'_, '_>,
    source_path: &[&str],
) -> Result<()> {
    let Some(compatibles) = discovered.compatible() else {
        return Ok(());
    };
    let node = discovered.node();
    match_desc!(first compatible in compatibles.all(), source_path; {
        plmt::COMPATIBLE => once(selected.timer.plmt, plmt::describe(platform, node)),
        plicsw::COMPATIBLE => once(selected.ipi.plicsw, plicsw::describe(platform, node)),
        aclint::MSWI_COMPATIBLE => node(
            selected.ipi.aclint,
            aclint::describe_mswi(platform, node)
        ),
        aclint::MTIMER_COMPATIBLE => node(
            selected.timer.aclint,
            aclint::describe_mtimer(platform, node)
        ),
        compatible if clint::supports(compatible) => node(
            selected.clint, clint::describe(platform, node)
        ),
        compatible if imsic::IMSIC_COMPATIBLES.contains(&compatible) => optional_node(
            selected.ipi.imsic, imsic::describe(platform, node)
        ),
        compatible if is_machine_aplic(node, compatible) => node(
            selected.irqchip.machine_aplic,
            platform.device_register(node)?.ok_or(Error::InvalidArgs),
            {
                selected.irqchip.machine_aplic_delegated =
                    node.property("riscv,delegate").is_some()
                        || node.property("riscv,delegation").is_some();
            }
        ),
        compatible if thead_plic::COMPATIBLES.contains(&compatible) => once(
            selected.irqchip.thead_plic,
            platform.device_register(node)?.ok_or(Error::InvalidArgs)
        )
    })
}

fn is_machine_aplic(node: FdtNode<'_, '_>, compatible: &str) -> bool {
    compatible == "riscv,aplic" && node.property("riscv,children").is_some()
}
