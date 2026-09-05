//! Interrupt-controller and reset-device discovery.

use runtime::memory::DeviceRegisterRange;
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
    visit_subtree(board, platform, root, None, &cpu_interrupt_controllers)
}

fn visit_subtree<'tree>(
    board: &mut BoardInfo,
    platform: &runtime::PlatformView<'tree>,
    node: &Node<'tree>,
    parent: Option<&Node<'tree>>,
    cpu_interrupt_controllers: &[imsic::CpuInterruptController],
) -> runtime::Result<()> {
    if !node_is_enabled(node) {
        return Ok(());
    }
    discover_node(board, platform, node, parent, cpu_interrupt_controllers)?;
    for child in node.nodes() {
        let child = child.deserialize::<Node<'tree>>();
        visit_subtree(
            board,
            platform,
            &child,
            Some(node),
            cpu_interrupt_controllers,
        )?;
    }
    Ok(())
}

fn discover_node(
    board: &mut BoardInfo,
    platform: &runtime::PlatformView<'_>,
    node: &Node<'_>,
    parent: Option<&Node<'_>>,
    cpu_interrupt_controllers: &[imsic::CpuInterruptController],
) -> runtime::Result<()> {
    let Some(compatibles) = compatible_strings(node) else {
        return Ok(());
    };
    let has_supported_pmic = compatibles
        .iter()
        .any(|compatible| driver::P1_PMIC_COMPATIBLES.contains(&compatible));
    let has_supported_mmio_device = compatibles
        .iter()
        .any(|compatible| is_supported_mmio_device(node, compatible));
    if !has_supported_pmic && !has_supported_mmio_device {
        return Ok(());
    }

    if has_supported_pmic {
        discover_pmic_reset(board, platform, node, parent)?;
    }
    if !has_supported_mmio_device {
        return Ok(());
    }

    let registers = platform
        .device_registers(node)?
        .ok_or(runtime::Error::InvalidArgs)?;
    let primary_register_range = registers[0];

    for compatible in compatibles.iter() {
        discover_clint(board, compatible, primary_register_range);
        discover_reset(board, compatible, primary_register_range);
        if driver::IMSIC_COMPATIBLES.contains(&compatible) && board.imsic.is_none() {
            board.imsic = imsic::discover(
                node,
                &registers,
                cpu_interrupt_controllers,
                &board.enabled_harts,
            )?;
        }
        if qemu_aplic::is_machine_domain(node, compatible) {
            board.machine_aplic = Some(primary_register_range);
        }
    }
    Ok(())
}

fn is_supported_mmio_device(node: &Node<'_>, compatible: &str) -> bool {
    driver::ClintKind::from_fdt(compatible).is_some()
        || driver::SIFIVE_TEST_COMPATIBLES.contains(&compatible)
        || driver::IMSIC_COMPATIBLES.contains(&compatible)
        || qemu_aplic::is_machine_domain(node, compatible)
}

fn discover_clint(board: &mut BoardInfo, compatible: &str, registers: DeviceRegisterRange) {
    if let Some(kind) = driver::ClintKind::from_fdt(compatible) {
        board.clint = Some((registers, kind));
    }
}

fn discover_reset(board: &mut BoardInfo, compatible: &str, registers: DeviceRegisterRange) {
    if driver::SIFIVE_TEST_COMPATIBLES.contains(&compatible) {
        board.reset = Some(registers);
    }
}

fn discover_pmic_reset<'tree>(
    board: &mut BoardInfo,
    platform: &runtime::PlatformView<'tree>,
    node: &Node<'tree>,
    parent: Option<&Node<'tree>>,
) -> runtime::Result<()> {
    // A PMIC child's `reg` value is an address on its parent I2C bus, not a
    // physical MMIO range, so it must not pass through `device_registers`.
    let addresses = node
        .get_prop("reg")
        .ok_or(runtime::Error::InvalidArgs)?
        .deserialize::<serde_device_tree::buildin::Reg>();
    let mut address_entries = addresses.iter();
    let address_entry = address_entries.next().ok_or(runtime::Error::InvalidArgs)?;
    if address_entries.next().is_some() {
        return Err(runtime::Error::InvalidArgs);
    }
    let address =
        driver::I2cAddress::new(address_entry.0.start).ok_or(runtime::Error::InvalidArgs)?;

    let parent = parent.ok_or(runtime::Error::InvalidArgs)?;
    let parent_compatibles = compatible_strings(parent).ok_or(runtime::Error::InvalidArgs)?;
    if !parent_compatibles
        .iter()
        .any(|compatible| driver::PMIC_I2C_COMPATIBLES.contains(&compatible))
    {
        return Err(runtime::Error::InvalidArgs);
    }
    let controller = platform
        .device_registers(parent)?
        .and_then(|ranges| ranges.first().copied())
        .ok_or(runtime::Error::InvalidArgs)?;
    board.pmic_reset = Some((controller, address));
    Ok(())
}
