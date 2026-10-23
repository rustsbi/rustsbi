//! Discovery of interrupt-controller resources.
//!
//! This pass records unbound interrupt descriptions in [`BoardInfo`]. Driver
//! selection and MMIO ownership remain in [`crate::driver`].

use alloc::vec::Vec;

use runtime::{Compatible, Error, FdtNode, PlatformView, Result, memory::DeviceRegisterRange};

use crate::devicetree::EnabledNode;
use crate::driver;
use crate::platform::info::{BoardInfo, ClintResource, MachineAplicHandoff};
use crate::platform::qemu_aplic;

use super::{aclint, cpu_intc::CpuInterruptController, imsic};

pub(super) fn discover_node(
    board: &mut BoardInfo,
    platform: &PlatformView<'_>,
    discovered: EnabledNode<'_, '_>,
    source_path: &[&str],
    cpu_interrupt_controllers: &[CpuInterruptController],
) -> Result<()> {
    let Some(compatibles) = discovered.compatible() else {
        return Ok(());
    };
    let node = discovered.node();

    let Some(controller) = InterruptController::from_compatibles(node, compatibles) else {
        return Ok(());
    };

    let resources = match &controller {
        InterruptController::Imsic => {
            let registers = platform.device_registers(node)?.ok_or(Error::InvalidArgs)?;
            Resources::Imsic(registers)
        }
        InterruptController::Aclint(kind) => {
            Resources::Aclint(kind.discover(platform, node, cpu_interrupt_controllers)?)
        }
        _ => Resources::Primary(platform.device_register(node)?.ok_or(Error::InvalidArgs)?),
    };
    controller.record(
        board,
        node,
        source_path,
        resources,
        cpu_interrupt_controllers,
    )
}

enum Resources {
    Imsic(Vec<DeviceRegisterRange>),
    Primary(DeviceRegisterRange),
    Aclint(aclint::Resource),
}

impl Resources {
    fn into_primary(self) -> Result<DeviceRegisterRange> {
        match self {
            Self::Primary(registers) => Ok(registers),
            _ => Err(Error::InvalidArgs),
        }
    }

    fn into_imsic(self) -> Result<Vec<DeviceRegisterRange>> {
        match self {
            Self::Imsic(registers) => Ok(registers),
            _ => Err(Error::InvalidArgs),
        }
    }
}

/// The interrupt driver selected for one device-tree node.
enum InterruptController {
    Plmt,
    Plicsw,
    Clint(driver::ClintKind),
    Imsic,
    Aclint(aclint::DeviceKind),
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
        } else if let Some(kind) = aclint::DeviceKind::from_compatible(compatible) {
            Some(Self::Aclint(kind))
        } else if let Some(kind) = driver::ClintKind::from_fdt(compatible) {
            Some(Self::Clint(kind))
        } else if driver::IMSIC_COMPATIBLES.contains(&compatible) {
            Some(Self::Imsic)
        } else if qemu_aplic::is_machine_domain(node, compatible) {
            let handoff = if qemu_aplic::is_delegated_machine_domain(node, compatible) {
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
        board: &mut BoardInfo,
        node: FdtNode<'_, '_>,
        source_path: &[&str],
        resources: Resources,
        cpu_interrupt_controllers: &[CpuInterruptController],
    ) -> Result<()> {
        match self {
            Self::Plmt => {
                if board.devices.interrupts.plmt.is_some() {
                    return Err(Error::InvalidArgs);
                }
                board.devices.interrupts.plmt = Some(resources.into_primary()?);
            }
            Self::Plicsw => {
                if board.devices.interrupts.plicsw.is_some() {
                    return Err(Error::InvalidArgs);
                }
                board.devices.interrupts.plicsw = Some(resources.into_primary()?);
            }
            Self::Aclint(kind) => match (kind, resources) {
                (aclint::DeviceKind::Mswi, Resources::Aclint(aclint::Resource::Mswi(mswi))) => {
                    if board.devices.interrupts.aclint_mswi.is_some() {
                        return Err(Error::InvalidArgs);
                    }
                    board.devices.interrupts.aclint_mswi = Some(mswi);
                }
                (
                    aclint::DeviceKind::Mtimer,
                    Resources::Aclint(aclint::Resource::Mtimer(mtimer)),
                ) => {
                    if board.devices.interrupts.aclint_mtimer.is_some() {
                        return Err(Error::InvalidArgs);
                    }
                    board.devices.interrupts.aclint_mtimer = Some(mtimer);
                }
                _ => return Err(Error::InvalidArgs),
            },
            Self::Clint(kind) => {
                board.devices.interrupts.set_clint(
                    ClintResource {
                        registers: resources.into_primary()?,
                        kind,
                    },
                    source_path,
                )?;
            }
            Self::Imsic => {
                let registers = resources.into_imsic()?;
                if let Some(imsic) = imsic::discover(node, &registers, cpu_interrupt_controllers)? {
                    if board.devices.interrupts.imsic().is_some() {
                        return Err(Error::InvalidArgs);
                    }
                    board.devices.interrupts.set_imsic(imsic, source_path)?;
                }
            }
            Self::MachineAplic(handoff) => {
                board.devices.interrupts.set_machine_aplic(
                    resources.into_primary()?,
                    source_path,
                    handoff,
                )?;
            }
            Self::TheadPlic => {
                if board.devices.interrupts.thead_plic.is_some() {
                    return Err(Error::InvalidArgs);
                }
                board.devices.interrupts.thead_plic = Some(resources.into_primary()?);
            }
        }
        Ok(())
    }
}
