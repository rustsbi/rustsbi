//! Discovery of interrupt-controller resources.
//!
//! This pass records unbound interrupt descriptions in [`BoardInfo`]. Driver
//! selection and MMIO ownership remain in [`crate::driver`].

use alloc::vec::Vec;

use runtime::{FdtNode, memory::DeviceRegisterRange};

use crate::devicetree::EnabledNode;
use crate::driver;
use crate::platform::info::{
    AclintMswi, AclintMtimer, BoardInfo, ClintResource, HartIndexMap, MachineAplicHandoff,
};
use crate::platform::qemu_aplic;

use super::imsic;

/// Interrupt identity a MSWI node uses for its machine software interrupts.
const MACHINE_SOFTWARE_INTERRUPT_ID: u32 = 3;
/// Interrupt identity a MTIMER node uses for its machine timer interrupts.
const MACHINE_TIMER_INTERRUPT_ID: u32 = 7;
/// Position of MTIMECMP in the legacy unlabelled ACLINT MTIMER representation.
const LEGACY_ACLINT_MTIMECMP_REG_INDEX: usize = 1;

/// Register resources collected for one interrupt-controller node.
struct Resources {
    /// Complete `reg` list, for nodes whose discovery needs every window.
    registers: Option<Vec<DeviceRegisterRange>>,
    /// Window used by single-window controllers, and the compare window of an
    /// ACLINT MTIMER device.
    primary: DeviceRegisterRange,
    /// Per-hart register indexing of an ACLINT device.
    hart_indices: HartIndexMap,
    /// MTIME window of an ACLINT MTIMER device, when it has one.
    time: Option<DeviceRegisterRange>,
}

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

    let resources = match &controller {
        InterruptController::Imsic => {
            let registers = platform
                .device_registers(node)?
                .ok_or(runtime::Error::InvalidArgs)?;
            let primary = registers
                .first()
                .copied()
                .ok_or(runtime::Error::InvalidArgs)?;
            Resources {
                registers: Some(registers),
                primary,
                hart_indices: HartIndexMap::new(),
                time: None,
            }
        }
        // An ACLINT MTIMER node carries separate MTIME and MTIMECMP windows.
        InterruptController::Mtimer => {
            let (compare, time) = aclint_mtimer_windows(platform, node)?;
            Resources {
                registers: None,
                primary: compare,
                hart_indices: aclint_hart_indices(
                    node,
                    cpu_interrupt_controllers,
                    MACHINE_TIMER_INTERRUPT_ID,
                )?,
                time,
            }
        }
        // An ACLINT MSWI node carries one IPI register per hart it serves.
        InterruptController::Mswi => Resources {
            registers: None,
            primary: platform
                .device_register(node)?
                .ok_or(runtime::Error::InvalidArgs)?,
            hart_indices: aclint_hart_indices(
                node,
                cpu_interrupt_controllers,
                MACHINE_SOFTWARE_INTERRUPT_ID,
            )?,
            time: None,
        },
        _ => Resources {
            registers: None,
            primary: platform
                .device_register(node)?
                .ok_or(runtime::Error::InvalidArgs)?,
            hart_indices: HartIndexMap::new(),
            time: None,
        },
    };
    controller.record(
        board,
        node,
        source_path,
        resources,
        cpu_interrupt_controllers,
    )
}

/// Reads the per-hart register indexing an ACLINT device assigns its harts.
///
/// An ACLINT device numbers its per-hart registers by the position of the hart
/// in the node's interrupt list, an index that "may or may not have any
/// relationship" with the hart ID, so it is read from `interrupts-extended`
/// instead of being assumed equal to the hart ID.
fn aclint_hart_indices(
    node: FdtNode<'_, '_>,
    controllers: &[imsic::CpuInterruptController],
    interrupt_id: u32,
) -> runtime::Result<HartIndexMap> {
    let cells = imsic::u32_cells(node, "interrupts-extended").ok_or(runtime::Error::InvalidArgs)?;
    if cells.len() % 2 != 0 {
        return Err(runtime::Error::InvalidArgs);
    }

    let mut hart_indices = HartIndexMap::new();
    for (index, interrupt) in cells.chunks_exact(2).enumerate() {
        if interrupt[1] != interrupt_id {
            return Err(runtime::Error::InvalidArgs);
        }
        let controller = controllers
            .iter()
            .find(|controller| controller.phandle == interrupt[0])
            .ok_or(runtime::Error::InvalidArgs)?;
        let index = u32::try_from(index).map_err(|_| runtime::Error::InvalidArgs)?;
        if !hart_indices.insert(controller.hart_id, index) {
            return Err(runtime::Error::InvalidArgs);
        }
    }
    if hart_indices.count() == 0 {
        return Err(runtime::Error::InvalidArgs);
    }
    Ok(hart_indices)
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
        resources: Resources,
        cpu_interrupt_controllers: &[imsic::CpuInterruptController],
    ) -> runtime::Result<()> {
        match self {
            Self::Plmt => {
                if board.devices.interrupts.plmt.is_some() {
                    return Err(runtime::Error::InvalidArgs);
                }
                board.devices.interrupts.plmt = Some(resources.primary);
            }
            Self::Plicsw => {
                if board.devices.interrupts.plicsw.is_some() {
                    return Err(runtime::Error::InvalidArgs);
                }
                board.devices.interrupts.plicsw = Some(resources.primary);
            }
            Self::Mswi => {
                if board.devices.interrupts.aclint_mswi.is_some() {
                    return Err(runtime::Error::InvalidArgs);
                }
                board.devices.interrupts.aclint_mswi = Some(AclintMswi {
                    registers: resources.primary,
                    hart_indices: resources.hart_indices,
                });
            }
            Self::Mtimer => {
                if board.devices.interrupts.aclint_mtimer.is_some() {
                    return Err(runtime::Error::InvalidArgs);
                }
                board.devices.interrupts.aclint_mtimer = Some(AclintMtimer {
                    compare: resources.primary,
                    time: resources.time,
                    hart_indices: resources.hart_indices,
                });
            }
            Self::Clint(kind) => {
                board.devices.interrupts.set_clint(
                    ClintResource {
                        registers: resources.primary,
                        kind,
                    },
                    source_path,
                )?;
            }
            Self::Imsic => {
                let registers = resources
                    .registers
                    .as_deref()
                    .ok_or(runtime::Error::InvalidArgs)?;
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
                    resources.primary,
                    source_path,
                    handoff,
                )?;
            }
            Self::TheadPlic => {
                if board.devices.interrupts.thead_plic.is_some() {
                    return Err(runtime::Error::InvalidArgs);
                }
                board.devices.interrupts.thead_plic = Some(resources.primary);
            }
        }
        Ok(())
    }
}

/// Returns the MTIMECMP window of an ACLINT MTIMER device, and its MTIME window
/// when the node provides one.
///
/// A node that labels its windows through `reg-names` is followed by name.
/// Unlabelled nodes fall back to `reg[1]`: the legacy representation emitted by
/// QEMU's `virt` machine, where `reg[0]` is MTIME and `reg[1]` is MTIMECMP.
/// Neither the ACLINT specification nor the devicetree bindings fix that order,
/// so the fallback accepts that compatibility convention rather than proving
/// it.
fn aclint_mtimer_windows(
    platform: &runtime::PlatformView<'_>,
    node: FdtNode<'_, '_>,
) -> runtime::Result<(DeviceRegisterRange, Option<DeviceRegisterRange>)> {
    if let Some(compare) = platform.device_register_by_name(node, "mtimecmp")? {
        // A labelled node may describe only the compare window; the firmware
        // then reads time from the architecture `time` CSR.
        return Ok((
            compare,
            platform
                .device_register_by_name(node, "mtime")
                .unwrap_or(None),
        ));
    }

    let registers = platform
        .device_registers(node)?
        .ok_or(runtime::Error::InvalidArgs)?;
    let time = registers
        .first()
        .copied()
        .ok_or(runtime::Error::InvalidArgs)?;
    let compare = registers
        .get(LEGACY_ACLINT_MTIMECMP_REG_INDEX)
        .copied()
        .ok_or(runtime::Error::InvalidArgs)?;
    Ok((compare, Some(time)))
}
