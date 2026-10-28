//! ACLINT Device Tree binding and normalized resource discovery.
//!
//! This module owns the platform-facing interpretation of ACLINT nodes. It
//! preserves the supported DT convention that an ACLINT node's
//! `interrupts-extended` pair order is its device-local hart register order.
//! The ACLINT specification defines those local indices, but their DT encoding
//! remains a platform-binding contract. The currently supported convention is
//! the one used by QEMU `virt,aclint=on`.

use riscv_aia::Iid;
use runtime::{Error, FdtNode, PlatformView, Result, hart::HartId, memory::DeviceRegisterRange};

use crate::devicetree::u32_cells;
use crate::platform::info::{AclintMswi, AclintMtimer, HartIndexMap};

use super::cpu_intc::CpuInterruptController;

const ACLINT_MAX_HARTS: usize = 4095;
const LEGACY_MTIMECMP_REG_INDEX: usize = 1;

const MSWI_COMPATIBLE: &str = "riscv,aclint-mswi";
const MTIMER_COMPATIBLE: &str = "riscv,aclint-mtimer";

/// ACLINT device classes exposed by the supported DT bindings.
#[derive(Clone, Copy)]
pub(super) enum DeviceKind {
    Mswi,
    Mtimer,
}

impl DeviceKind {
    pub(super) fn from_compatible(compatible: &str) -> Option<Self> {
        if compatible == MSWI_COMPATIBLE {
            Some(Self::Mswi)
        } else if compatible == MTIMER_COMPATIBLE {
            Some(Self::Mtimer)
        } else {
            None
        }
    }

    pub(super) fn discover(
        self,
        platform: &PlatformView<'_>,
        node: FdtNode<'_, '_>,
        controllers: &[CpuInterruptController],
    ) -> Result<Resource> {
        match self {
            Self::Mswi => {
                let registers = platform.device_register(node)?.ok_or(Error::InvalidArgs)?;
                let hart_indices = hart_indices(node, controllers, Iid::MSOFT, ACLINT_MAX_HARTS)?;
                Ok(Resource::Mswi(AclintMswi {
                    registers,
                    hart_indices,
                }))
            }
            Self::Mtimer => {
                let (compare, time) = mtimer_windows(platform, node)?;
                let hart_indices = hart_indices(node, controllers, Iid::MTIMER, ACLINT_MAX_HARTS)?;
                Ok(Resource::Mtimer(AclintMtimer {
                    compare,
                    time,
                    hart_indices,
                }))
            }
        }
    }
}

/// A normalized ACLINT resource produced by the DT binding adapter.
pub(super) enum Resource {
    Mswi(AclintMswi),
    Mtimer(AclintMtimer),
}

/// Parses the device-local hart slots from an ACLINT node's interrupt wiring.
///
/// The pair's position is the device's physical register index under the
/// supported ACLINT DT convention. Disabled CPU references still consume a
/// slot, but are omitted from the Runtime-facing map.
fn hart_indices(
    node: FdtNode<'_, '_>,
    controllers: &[CpuInterruptController],
    iid: Iid,
    max_harts: usize,
) -> Result<HartIndexMap> {
    let cells = u32_cells(node, "interrupts-extended").ok_or(Error::InvalidArgs)?;
    if cells.is_empty() || !cells.len().is_multiple_of(2) {
        return Err(Error::InvalidArgs);
    }
    let hart_count = cells.len() / 2;
    if hart_count > max_harts {
        return Err(Error::InvalidArgs);
    }

    let mut hart_indices = HartIndexMap::with_harts(hart_count);
    for (index, interrupt) in cells.chunks_exact(2).enumerate() {
        if interrupt[1] != u32::from(iid.number()) {
            return Err(Error::InvalidArgs);
        }
        let controller = controllers
            .iter()
            .find(|controller| controller.phandle() == interrupt[0])
            .ok_or(Error::InvalidArgs)?;
        let index = u32::try_from(index).map_err(|_| Error::InvalidArgs)?;
        let Ok(hart) = HartId::from_raw(controller.hart_id()) else {
            continue;
        };
        if !controller.enabled() {
            return Err(Error::InvalidArgs);
        }
        if !hart_indices.insert(hart, index) {
            return Err(Error::InvalidArgs);
        }
    }
    if !hart_indices.covers_all_enabled() {
        return Err(Error::InvalidArgs);
    }
    Ok(hart_indices)
}

/// Resolves MTIME and MTIMECMP windows.
///
/// Named bindings use their declared names. For the unlabelled ACLINT
/// representation emitted by QEMU, `reg[0]` is MTIME and `reg[1]` is MTIMECMP.
fn mtimer_windows(
    platform: &PlatformView<'_>,
    node: FdtNode<'_, '_>,
) -> Result<(DeviceRegisterRange, Option<DeviceRegisterRange>)> {
    if node.property("reg-names").is_some() {
        let compare = platform
            .device_register_by_name(node, "mtimecmp")?
            .ok_or(Error::InvalidArgs)?;
        let time = if named_property_contains(node, "mtime")? {
            Some(
                platform
                    .device_register_by_name(node, "mtime")?
                    .ok_or(Error::InvalidArgs)?,
            )
        } else {
            None
        };
        return Ok((compare, time));
    }

    let registers = platform.device_registers(node)?.ok_or(Error::InvalidArgs)?;
    let time = registers.first().copied().ok_or(Error::InvalidArgs)?;
    let compare = registers
        .get(LEGACY_MTIMECMP_REG_INDEX)
        .copied()
        .ok_or(Error::InvalidArgs)?;
    Ok((compare, Some(time)))
}

fn named_property_contains(node: FdtNode<'_, '_>, name: &str) -> Result<bool> {
    let value = node.property("reg-names").ok_or(Error::InvalidArgs)?.value;
    if !value.ends_with(&[0]) {
        return Err(Error::InvalidArgs);
    }
    Ok(value
        .split(|byte| *byte == 0)
        .any(|entry| entry == name.as_bytes()))
}
