//! Timer, IPI, and interrupt-controller resources from the device tree.

use runtime::{Error, FdtNode, PlatformView, Result, memory::RegRange};

use super::{aclint, clint, imsic, mswi, mtimer, plicsw, plmt, select_once, thead_plic};
use crate::devicetree::NodeSelection;

/// Timer resources retained until platform initialization.
#[derive(Default)]
pub(crate) struct TimerSelection {
    pub(crate) aclint: Option<mtimer::Desc>,
    pub(crate) clint: Option<mtimer::Desc>,
    pub(crate) plmt: Option<plmt::Desc>,
}

/// IPI resources retained until platform initialization.
#[derive(Default)]
pub(crate) struct IpiSelection {
    pub(crate) aclint: Option<mswi::Desc>,
    pub(crate) clint: Option<NodeSelection<mswi::Desc>>,
    pub(crate) imsic: Option<NodeSelection<imsic::Desc>>,
    pub(crate) plicsw: Option<plicsw::Desc>,
}

/// Interrupt-controller resources, including the machine APLIC's handoff path.
#[derive(Default)]
pub(crate) struct IrqchipSelection {
    pub(crate) machine_aplic: Option<NodeSelection<RegRange>>,
    pub(crate) machine_aplic_delegated: bool,
    pub(crate) thead_plic: Option<RegRange>,
}

/// Parsed interrupt resources pending MMIO acquisition.
#[derive(Default)]
pub(crate) struct Selection {
    pub(crate) timer: TimerSelection,
    pub(crate) ipi: IpiSelection,
    pub(crate) irqchip: IrqchipSelection,
}

/// Parses enabled nodes in device-tree order before initializing any device.
pub(crate) fn describe(platform: &PlatformView<'_>) -> Result<Selection> {
    let mut selected = Selection::default();
    platform.try_for_each_enabled_node(|node, _, path| {
        describe_node(&mut selected, platform, node, path)
    })?;
    Ok(selected)
}

fn describe_node<'tree>(
    selected: &mut Selection,
    platform: &PlatformView<'tree>,
    node: FdtNode<'_, 'tree>,
    source_path: &[&str],
) -> Result<()> {
    let Some(compatibles) = node.compatible() else {
        return Ok(());
    };
    for compatible in compatibles.all() {
        match compatible {
            plmt::COMPATIBLE => {
                let description = plmt::describe(platform, node)?;
                select_once(&mut selected.timer.plmt, description)?;
            }
            aclint::MTIMER_COMPATIBLE => {
                let description = aclint::describe_mtimer(platform, node)?;
                select_once(&mut selected.timer.aclint, description)?;
            }
            plicsw::COMPATIBLE => {
                let description = plicsw::describe(platform, node)?;
                select_once(&mut selected.ipi.plicsw, description)?;
            }
            aclint::MSWI_COMPATIBLE => {
                let description = aclint::describe_mswi(platform, node)?;
                select_once(&mut selected.ipi.aclint, description)?;
            }
            value if imsic::IMSIC_COMPATIBLES.contains(&value) => {
                // A supervisor-only IMSIC still consumes this match.
                if let Some(description) = imsic::describe(platform, node)? {
                    select_once(
                        &mut selected.ipi.imsic,
                        NodeSelection::new(description, source_path),
                    )?;
                }
            }
            value if clint::COMPATIBLES.contains(&value) => {
                let clint::Desc { ipi, timer } = clint::describe(platform, node, value)?;
                select_once(
                    &mut selected.ipi.clint,
                    NodeSelection::new(ipi, source_path),
                )?;
                selected.timer.clint = Some(timer);
            }
            "riscv,aplic" if node.property("riscv,children").is_some() => {
                let registers = platform.device_register(node)?.ok_or(Error::InvalidArgs)?;
                select_once(
                    &mut selected.irqchip.machine_aplic,
                    NodeSelection::new(registers, source_path),
                )?;
                selected.irqchip.machine_aplic_delegated =
                    node.property("riscv,delegate").is_some()
                        || node.property("riscv,delegation").is_some();
            }
            value if thead_plic::COMPATIBLES.contains(&value) => {
                let registers = platform.device_register(node)?.ok_or(Error::InvalidArgs)?;
                select_once(&mut selected.irqchip.thead_plic, registers)?;
            }
            _ => continue,
        }
        break;
    }
    Ok(())
}
