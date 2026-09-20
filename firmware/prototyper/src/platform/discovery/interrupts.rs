//! Discovery of interrupt-controller resources.
//!
//! This pass records unbound interrupt descriptions in [`BoardInfo`]. Driver
//! selection and MMIO ownership remain in [`crate::driver`].

use runtime::node_is_enabled;
use serde_device_tree::buildin::Node;

use crate::devicetree::compatible_strings;
use crate::driver;
use crate::platform::info::BoardInfo;
use crate::platform::qemu_aplic;

use super::imsic;

pub(super) fn discover(
    board: &mut BoardInfo,
    platform: &runtime::PlatformView<'_>,
) -> runtime::Result<()> {
    let root = platform.root();
    let cpu_interrupt_controllers = imsic::cpu_interrupt_controllers(root)?;
    visit_subtree(board, platform, root, &cpu_interrupt_controllers)
}

fn visit_subtree<'tree>(
    board: &mut BoardInfo,
    platform: &runtime::PlatformView<'tree>,
    node: &Node<'tree>,
    cpu_interrupt_controllers: &[imsic::CpuInterruptController],
) -> runtime::Result<()> {
    if !node_is_enabled(node) {
        return Ok(());
    }
    discover_node(board, platform, node, cpu_interrupt_controllers)?;
    for child in node.nodes() {
        let child = child.deserialize::<Node<'tree>>();
        visit_subtree(board, platform, &child, cpu_interrupt_controllers)?;
    }
    Ok(())
}

fn discover_node(
    board: &mut BoardInfo,
    platform: &runtime::PlatformView<'_>,
    node: &Node<'_>,
    cpu_interrupt_controllers: &[imsic::CpuInterruptController],
) -> runtime::Result<()> {
    let Some(compatibles) = compatible_strings(node) else {
        return Ok(());
    };

    let has_interrupt_device = compatibles
        .iter()
        .any(|compatible| is_interrupt_device(node, compatible));
    if !has_interrupt_device {
        return Ok(());
    }

    let registers = platform
        .device_registers(node)?
        .ok_or(runtime::Error::InvalidArgs)?;
    let primary_register_range = registers
        .first()
        .copied()
        .ok_or(runtime::Error::InvalidArgs)?;
    for compatible in compatibles.iter() {
        let slot = match compatible {
            driver::PLMT_COMPATIBLE => Some(&mut board.devices.interrupts.plmt),
            driver::SUNXI_PLICSW_COMPATIBLE => Some(&mut board.devices.interrupts.plicsw),
            _ => None,
        };
        if let Some(slot) = slot
            && slot.replace(primary_register_range).is_some()
        {
            return Err(runtime::Error::InvalidArgs);
        }
        if let Some(kind) = driver::ClintKind::from_fdt(compatible) {
            board.devices.interrupts.clint = Some((primary_register_range, kind));
        }
        if driver::IMSIC_COMPATIBLES.contains(&compatible)
            && board.devices.interrupts.imsic.is_none()
        {
            board.devices.interrupts.imsic = imsic::discover(
                node,
                &registers,
                cpu_interrupt_controllers,
                &board.harts.enabled,
            )?;
        }
        if qemu_aplic::is_machine_domain(node, compatible) {
            board.devices.interrupts.machine_aplic = Some(primary_register_range);
        }
        if driver::THEAD_PLIC_COMPATIBLES.contains(&compatible) {
            board.devices.interrupts.thead_plic = Some(primary_register_range);
        }
    }
    Ok(())
}

fn is_interrupt_device(node: &Node<'_>, compatible: &str) -> bool {
    compatible == driver::PLMT_COMPATIBLE
        || compatible == driver::SUNXI_PLICSW_COMPATIBLE
        || driver::ClintKind::from_fdt(compatible).is_some()
        || driver::IMSIC_COMPATIBLES.contains(&compatible)
        || driver::THEAD_PLIC_COMPATIBLES.contains(&compatible)
        || qemu_aplic::is_machine_domain(node, compatible)
}
