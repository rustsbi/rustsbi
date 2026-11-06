//! ACLINT MSWI and MTIMER Device Tree bindings and descriptions.
//!
//! # References
//!
//! - Specification: [RISC-V ACLINT 1.0](https://github.com/riscvarchive/riscv-aclint/blob/4e570bfd3201f2c09e5afd290b5091526b0f099a/riscv-aclint.adoc) —
//!   sections 2 and 3 define the MTIMER and MSWI register maps and
//!   device-local HART indices.
//! - QEMU virt binding convention:
//!   [ACLINT FDT generation](https://github.com/qemu/qemu/blob/99e54ab5e7a6efc945af6d5661842155d1f3fc7a/hw/riscv/virt.c) —
//!   the unlabelled MTIMER `reg` order and the `interrupts-extended`
//!   pair order used for device-local register slots.
//! - Reference implementation:
//!   [OpenSBI MTIMER FDT driver](https://github.com/riscv-software-src/opensbi/blob/35511bc6ee1c9c17b6a89b44c52e2044bb51b979/lib/utils/timer/fdt_timer_mtimer.c)
//!   and [MSWI FDT driver](https://github.com/riscv-software-src/opensbi/blob/35511bc6ee1c9c17b6a89b44c52e2044bb51b979/lib/utils/ipi/fdt_ipi_mswi.c)
//!   define the supported compatible strings and resource binding.
//!
//! The ACLINT specification defines device-local hart indices. This adapter
//! supports the QEMU `virt,aclint=on` convention, where the position of each
//! `interrupts-extended` pair is that hart's slot in the device register map.

use runtime::{Error, FdtNode, PlatformView, Result, memory::RegRange};

use crate::devicetree::resolve_interrupts_extended;

use super::{
    hart_slot::HartSlotMap,
    mswi,
    mtimer::{self, CompareAccess},
};

const ACLINT_MAX_HARTS: usize = 4095;
const LEGACY_MTIMECMP_REG_INDEX: usize = 1;

/// ACLINT software interrupts use local interrupt cause 3.
const MSWI_INTERRUPT_ID: u32 = 3;
/// ACLINT timer interrupts use local interrupt cause 7.
const MTIMER_INTERRUPT_ID: u32 = 7;

pub(crate) const MSWI_COMPATIBLE: &str = "riscv,aclint-mswi";
pub(crate) const MTIMER_COMPATIBLE: &str = "riscv,aclint-mtimer";

pub(super) fn describe_mswi(
    platform: &PlatformView<'_>,
    node: FdtNode<'_, '_>,
) -> Result<mswi::Desc> {
    let registers = platform.device_register(node)?.ok_or(Error::InvalidArgs)?;
    let hart_slots = hart_slots(platform, node, MSWI_INTERRUPT_ID)?;
    Ok(mswi::Desc {
        registers,
        hart_slots,
        kind: "ACLINT MSWI",
    })
}

pub(super) fn describe_mtimer(
    platform: &PlatformView<'_>,
    node: FdtNode<'_, '_>,
) -> Result<mtimer::Desc> {
    let (compare, time) = mtimer_windows(platform, node)?;
    let hart_slots = hart_slots(platform, node, MTIMER_INTERRUPT_ID)?;
    Ok(mtimer::Desc {
        compare,
        time,
        hart_slots,
        access: CompareAccess::Native,
        kind: "ACLINT MTIMER",
    })
}

fn hart_slots(
    platform: &PlatformView<'_>,
    node: FdtNode<'_, '_>,
    interrupt_id: u32,
) -> Result<HartSlotMap> {
    let sources = resolve_interrupts_extended(platform, node)?;
    let hart_count = sources.len();
    if hart_count > ACLINT_MAX_HARTS {
        return Err(Error::InvalidArgs);
    }

    let mut hart_slots = HartSlotMap::table(hart_count);
    for source in sources {
        if source.interrupt_id != interrupt_id {
            return Err(Error::InvalidArgs);
        }
        let Some(hart) = source.hart else {
            continue;
        };
        if !source.controller_enabled {
            return Err(Error::InvalidArgs);
        }
        if !hart_slots.insert(hart, source.slot) {
            return Err(Error::InvalidArgs);
        }
    }
    if !hart_slots.covers_all_enabled() {
        return Err(Error::InvalidArgs);
    }
    Ok(hart_slots)
}

/// Resolves MTIME and MTIMECMP windows.
///
/// Named bindings use their declared names. QEMU's unlabelled ACLINT binding
/// uses `reg[0]` for MTIME and `reg[1]` for MTIMECMP.
fn mtimer_windows(
    platform: &PlatformView<'_>,
    node: FdtNode<'_, '_>,
) -> Result<(RegRange, Option<RegRange>)> {
    if node.property("reg-names").is_some() {
        let compare = platform
            .device_register_by_name(node, "mtimecmp")?
            .ok_or(Error::InvalidArgs)?;
        let time = if register_name_present(node, "mtime")? {
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

fn register_name_present(node: FdtNode<'_, '_>, name: &str) -> Result<bool> {
    let value = node.property("reg-names").ok_or(Error::InvalidArgs)?.value;
    if !value.ends_with(&[0]) {
        return Err(Error::InvalidArgs);
    }
    Ok(value
        .split(|byte| *byte == 0)
        .any(|entry| entry == name.as_bytes()))
}
