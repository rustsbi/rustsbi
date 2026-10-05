//! Discovery of interrupt-controller resources.
//!
//! This pass records unbound interrupt descriptions in [`BoardInfo`]. Driver
//! selection and MMIO ownership remain in [`crate::driver`].

use runtime::{FdtNode, memory::DeviceRegisterRange};

use crate::devicetree::EnabledNode;
use crate::driver;
use crate::platform::info::{BoardInfo, ClintResource, MachineAplicHandoff};
use crate::platform::qemu_aplic;

use super::imsic;

pub(super) fn discover_node(
    board: &mut BoardInfo,
    platform: &runtime::PlatformView<'_>,
    discovered: EnabledNode<'_, '_>,
    source_path: &[&str],
    cpu_interrupt_controllers: &[imsic::CpuInterruptController],
) -> runtime::Result<()> {
    let Some(compatibles) = discovered.compatible() else {
        return Ok(());
    };
    let node = discovered.node();

    let Some(controller) = InterruptController::from_compatibles(node, compatibles) else {
        return Ok(());
    };

    let (registers, primary_register_range) = match &controller {
        // ACLINT MTIMER splits `mtime` and `mtimecmp` into separate `reg` entries.
        InterruptController::Imsic | InterruptController::Mtimer => {
            let registers = platform
                .device_registers(node)?
                .ok_or(runtime::Error::InvalidArgs)?;
            let primary = registers
                .first()
                .copied()
                .ok_or(runtime::Error::InvalidArgs)?;
            (Some(registers), primary)
        }
        _ => (
            None,
            platform
                .device_register(node)?
                .ok_or(runtime::Error::InvalidArgs)?,
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
    Clint(driver::ClintKind),
    Imsic,
    Mswi,
    Mtimer,
    MachineAplic(MachineAplicHandoff),
    TheadPlic,
}

impl InterruptController {
    /// Selects the first supported `compatible`, following the fallback order
    /// used by OpenSBI's FDT driver dispatcher.
    ///
    /// See <https://github.com/riscv-software-src/opensbi/blob/master/lib/utils/fdt/fdt_driver.c>.
    fn from_compatibles(
        node: FdtNode<'_, '_>,
        compatibles: runtime::Compatible<'_>,
    ) -> Option<Self> {
        compatibles
            .all()
            .find_map(|compatible| Self::from_compatible(node, compatible))
    }

    fn from_compatible(node: FdtNode<'_, '_>, compatible: &str) -> Option<Self> {
        if compatible == driver::PLMT_COMPATIBLE {
            Some(Self::Plmt)
        } else if compatible == driver::SUNXI_PLICSW_COMPATIBLE {
            Some(Self::Plicsw)
        } else if compatible == driver::ACLINT_MSWI_COMPATIBLE {
            Some(Self::Mswi)
        } else if compatible == driver::ACLINT_MTIMER_COMPATIBLE {
            Some(Self::Mtimer)
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
        registers: Option<&[DeviceRegisterRange]>,
        primary_register_range: DeviceRegisterRange,
        cpu_interrupt_controllers: &[imsic::CpuInterruptController],
    ) -> runtime::Result<()> {
        match self {
            Self::Plmt => {
                if board.devices.interrupts.plmt.is_some() {
                    return Err(runtime::Error::InvalidArgs);
                }
                board.devices.interrupts.plmt = Some(primary_register_range);
            }
            Self::Plicsw => {
                if board.devices.interrupts.plicsw.is_some() {
                    return Err(runtime::Error::InvalidArgs);
                }
                board.devices.interrupts.plicsw = Some(primary_register_range);
            }
            Self::Mswi => {
                if board.devices.interrupts.aclint_mswi.is_some() {
                    return Err(runtime::Error::InvalidArgs);
                }
                board.devices.interrupts.aclint_mswi = Some(primary_register_range);
            }
            Self::Mtimer => {
                if board.devices.interrupts.aclint_mtimer.is_some() {
                    return Err(runtime::Error::InvalidArgs);
                }
                let registers = registers.ok_or(runtime::Error::InvalidArgs)?;
                board.devices.interrupts.aclint_mtimer = Some(base_register_range(registers)?);
            }
            Self::Clint(kind) => {
                board.devices.interrupts.set_clint(
                    ClintResource {
                        registers: primary_register_range,
                        kind,
                    },
                    source_path,
                )?;
            }
            Self::Imsic => {
                let registers = registers.ok_or(runtime::Error::InvalidArgs)?;
                if let Some(imsic) = imsic::discover(
                    node,
                    registers,
                    cpu_interrupt_controllers,
                    &board.harts.enabled,
                )? {
                    if board.devices.interrupts.imsic().is_some() {
                        return Err(runtime::Error::InvalidArgs);
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
                    return Err(runtime::Error::InvalidArgs);
                }
                board.devices.interrupts.thead_plic = Some(primary_register_range);
            }
        }
        Ok(())
    }
}

/// Returns the lowest-address `reg` window, the ACLINT MTIMER base.
fn base_register_range(registers: &[DeviceRegisterRange]) -> runtime::Result<DeviceRegisterRange> {
    registers
        .iter()
        .copied()
        .min_by_key(|range| range.start().as_usize())
        .ok_or(runtime::Error::InvalidArgs)
}
