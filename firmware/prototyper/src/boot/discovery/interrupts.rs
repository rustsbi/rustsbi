//! Discovery of interrupt-controller resources.
//!
//! This pass records unbound interrupt descriptions in [`PlatformInfo`]. Driver
//! selection and MMIO ownership remain in [`crate::driver`].

use runtime::{Compatible, Error, FdtNode, PlatformView, Result, memory::DeviceRegisterRange};

use crate::devicetree::EnabledNode;
use crate::driver;
use crate::driver::irqchip::aplic;
use crate::info::{MachineAplicHandoff, PlatformInfo};

use super::imsic;

pub(super) fn discover_node(
    board: &mut PlatformInfo,
    platform: &PlatformView<'_>,
    discovered: EnabledNode<'_, '_>,
    source_path: &[&str],
    cpu_interrupt_controllers: &[imsic::CpuInterruptController],
) -> Result<()> {
    let Some(compatibles) = discovered.compatible() else {
        return Ok(());
    };
    let node = discovered.node();

    let Some(controller) = InterruptController::from_compatibles(node, compatibles) else {
        return Ok(());
    };

    let (registers, primary_register_range) = match &controller {
        InterruptController::Imsic => {
            let registers = platform.device_registers(node)?.ok_or(Error::InvalidArgs)?;
            let primary = registers.first().copied().ok_or(Error::InvalidArgs)?;
            (Some(registers), primary)
        }
        _ => (
            None,
            platform.device_register(node)?.ok_or(Error::InvalidArgs)?,
        ),
    };
    controller.record(
        board,
        node,
        source_path,
        registers.as_deref(),
        primary_register_range,
        cpu_interrupt_controllers,
    )
}

/// The interrupt driver selected for one device-tree node.
enum InterruptController {
    Plmt,
    Plicsw,
    Clint,
    Imsic,
    MachineAplic(MachineAplicHandoff),
    TheadPlic,
}

impl InterruptController {
    /// Selects the first supported `compatible`, following the fallback order
    /// used by OpenSBI's FDT driver dispatcher.
    ///
    /// See <https://github.com/riscv-software-src/opensbi/blob/master/lib/utils/fdt/fdt_driver.c>.
    fn from_compatibles(node: FdtNode<'_, '_>, compatibles: Compatible<'_>) -> Option<Self> {
        compatibles
            .all()
            .find_map(|compatible| Self::from_compatible(node, compatible))
    }

    fn from_compatible(node: FdtNode<'_, '_>, compatible: &str) -> Option<Self> {
        if compatible == driver::PLMT_COMPATIBLE {
            Some(Self::Plmt)
        } else if compatible == driver::SUNXI_PLICSW_COMPATIBLE {
            Some(Self::Plicsw)
        } else if driver::clint::COMPATIBLES.contains(&compatible) {
            Some(Self::Clint)
        } else if driver::IMSIC_COMPATIBLES.contains(&compatible) {
            Some(Self::Imsic)
        } else if aplic::is_machine_domain(node, compatible) {
            let handoff = if aplic::is_delegated_machine_domain(node, compatible) {
                MachineAplicHandoff::Hide
            } else {
                MachineAplicHandoff::KeepVisible
            };
            Some(Self::MachineAplic(handoff))
        } else if driver::THEAD_PLIC_COMPATIBLES.contains(&compatible) {
            Some(Self::TheadPlic)
        } else {
            None
        }
    }

    fn record(
        self,
        board: &mut PlatformInfo,
        node: FdtNode<'_, '_>,
        source_path: &[&str],
        registers: Option<&[DeviceRegisterRange]>,
        primary_register_range: DeviceRegisterRange,
        cpu_interrupt_controllers: &[imsic::CpuInterruptController],
    ) -> Result<()> {
        match self {
            Self::Plmt => {
                if board.devices.interrupts.plmt.is_some() {
                    return Err(Error::InvalidArgs);
                }
                board.devices.interrupts.plmt = Some(primary_register_range);
            }
            Self::Plicsw => {
                if board.devices.interrupts.plicsw.is_some() {
                    return Err(Error::InvalidArgs);
                }
                board.devices.interrupts.plicsw = Some(primary_register_range);
            }
            Self::Clint => {
                board
                    .devices
                    .interrupts
                    .set_clint(primary_register_range, source_path)?;
            }
            Self::Imsic => {
                let registers = registers.ok_or(Error::InvalidArgs)?;
                if let Some(imsic) = imsic::discover(node, registers, cpu_interrupt_controllers)? {
                    if board.devices.interrupts.imsic().is_some() {
                        return Err(Error::InvalidArgs);
                    }
                    board.devices.interrupts.set_imsic(imsic, source_path)?;
                }
            }
            Self::MachineAplic(handoff) => {
                board.devices.interrupts.set_machine_aplic(
                    primary_register_range,
                    source_path,
                    handoff,
                )?;
            }
            Self::TheadPlic => {
                if board.devices.interrupts.thead_plic.is_some() {
                    return Err(Error::InvalidArgs);
                }
                board.devices.interrupts.thead_plic = Some(primary_register_range);
            }
        }
        Ok(())
    }
}
