//! Discovery of machine-level IMSIC interrupt files.
//!
//! Specification: [RISC-V AIA 1.0], section 3.6, defines address layout and
//! interrupt-file semantics. Devicetree binding: the pinned [IMSIC binding]
//! defines property bounds and defaults.
//!
//! [RISC-V AIA 1.0]: https://docs.riscv.org/reference/aia/_attachments/riscv-interrupts.pdf
//! [IMSIC binding]: https://github.com/torvalds/linux/blob/a500db7819c50db59e55f1b4fa1c3baa5a2616f3/Documentation/devicetree/bindings/interrupt-controller/riscv%2Cimsics.yaml

use alloc::vec::Vec;
use core::mem::size_of;

use riscv_aia::Iid;
use runtime::{
    memory::{DeviceRegisterRange, PhysAddrRange},
    node_is_enabled,
};
use serde_device_tree::buildin::Node;

use crate::cfg::NUM_HART_MAX;
use crate::devicetree::{Cpu, compatible_strings};
use crate::driver;

use super::super::info::{HartEnableList, ImsicAddressLayout, ImsicInfo};

const MACHINE_EXTERNAL_INTERRUPT_ID: u32 = 11;
const MIN_INTERRUPT_IDENTITIES: u32 = 63;
const MAX_INTERRUPT_IDENTITIES: u32 = 2047;
const MAX_GUEST_INDEX_BITS: u32 = 7;
const MAX_HART_INDEX_BITS: u32 = 15;
const MAX_GROUP_INDEX_BITS: u32 = 7;
const MAX_GROUP_INDEX_SHIFT: u32 = 55;
const DEFAULT_GROUP_INDEX_SHIFT: u32 = 24;
const FIRMWARE_IPI_IDENTITY: u16 = 1;

#[derive(Clone, Copy)]
pub(super) struct CpuInterruptController {
    phandle: u32,
    hart_id: usize,
}

struct MachineInterruptFile {
    hart_id: usize,
    file_index: u32,
}

pub(super) fn cpu_interrupt_controllers(
    root: &Node<'_>,
) -> runtime::Result<Vec<CpuInterruptController>> {
    let mut controllers = Vec::new();
    let Some(cpus) = root.find("/cpus") else {
        return Ok(controllers);
    };

    for cpu_item in cpus.nodes() {
        let (node_name, _) = cpu_item.get_parsed_name();
        if node_name != "cpu" {
            continue;
        }
        let cpu_node = cpu_item.deserialize::<Node>();
        if !node_is_enabled(&cpu_node) {
            continue;
        }
        let cpu = cpu_item.deserialize::<Cpu>();
        let hart_id = cpu
            .reg
            .iter()
            .next()
            .map(|register| register.0.start)
            .ok_or(runtime::Error::InvalidArgs)?;
        for child_item in cpu_node.nodes() {
            let (child_name, _) = child_item.get_parsed_name();
            if child_name != "interrupt-controller" {
                continue;
            }
            let child = child_item.deserialize::<Node>();
            if !is_cpu_interrupt_controller(&child) {
                continue;
            }
            if let Some(phandle) = phandle(&child) {
                controllers.push(CpuInterruptController { phandle, hart_id });
            }
        }
    }
    Ok(controllers)
}

pub(super) fn discover(
    node: &Node<'_>,
    register_ranges: &[DeviceRegisterRange],
    cpu_interrupt_controllers: &[CpuInterruptController],
    enabled_harts: &HartEnableList,
) -> runtime::Result<Option<ImsicInfo>> {
    let first_register_range = register_ranges.first().ok_or(runtime::Error::InvalidArgs)?;
    if register_ranges
        .iter()
        .any(|range| !range.has_aligned_bounds(driver::IMSIC_FILE_SPAN))
    {
        return Err(runtime::Error::InvalidArgs);
    }

    let machine_base = first_register_range.start();
    let num_ids = interrupt_identity_count(node)?;
    let machine_files = machine_interrupt_files(node, cpu_interrupt_controllers)?;
    if machine_files.is_empty() {
        debug!(
            "IMSIC: node at 0x{:x} is not wired to MachineExternal, skipping",
            machine_base.as_usize()
        );
        return Ok(None);
    }

    let machine_file_count =
        u32::try_from(machine_files.len()).map_err(|_| runtime::Error::InvalidArgs)?;
    let default_hart_index_bits = machine_file_count
        .checked_sub(1)
        .map_or(0, |last| u32::BITS - last.leading_zeros());
    let hart_index_bits =
        u32_property(node, "riscv,hart-index-bits").unwrap_or(default_hart_index_bits);
    let group_index_bits = u32_property(node, "riscv,group-index-bits").unwrap_or(0);
    let group_index_shift =
        u32_property(node, "riscv,group-index-shift").unwrap_or(DEFAULT_GROUP_INDEX_SHIFT);
    let guest_index_bits = u32_property(node, "riscv,guest-index-bits").unwrap_or(0);
    let file_page_shift = driver::IMSIC_FILE_SPAN.trailing_zeros();
    validate_topology(
        guest_index_bits,
        hart_index_bits,
        group_index_bits,
        group_index_shift,
        file_page_shift,
    )?;

    let ipi_iid =
        Iid::new(FIRMWARE_IPI_IDENTITY).expect("BUG: firmware IPI identity must be nonzero");
    if ipi_iid.number() >= num_ids {
        return Err(runtime::Error::InvalidArgs);
    }

    let layout = ImsicAddressLayout::new(
        machine_base,
        hart_index_bits,
        group_index_shift,
        file_page_shift + guest_index_bits,
    );
    let hart_files = map_hart_files(
        &layout,
        register_ranges,
        &machine_files,
        enabled_harts,
        group_index_bits,
    )?;

    Ok(Some(ImsicInfo {
        layout,
        num_ids,
        ipi_iid,
        hart_files,
    }))
}

fn is_cpu_interrupt_controller(node: &Node<'_>) -> bool {
    node_is_enabled(node)
        && compatible_strings(node).is_some_and(|compatibles| {
            compatibles
                .iter()
                .any(|device_id| device_id == "riscv,cpu-intc")
        })
}

fn phandle(node: &Node<'_>) -> Option<u32> {
    node.get_prop("phandle")
        .or_else(|| node.get_prop("linux,phandle"))
        .map(|property| property.deserialize::<u32>())
}

fn interrupt_identity_count(node: &Node<'_>) -> runtime::Result<u16> {
    let num_ids = u32_property(node, "riscv,num-ids").ok_or(runtime::Error::InvalidArgs)?;
    if !(MIN_INTERRUPT_IDENTITIES..=MAX_INTERRUPT_IDENTITIES).contains(&num_ids) {
        return Err(runtime::Error::InvalidArgs);
    }
    Ok(num_ids as u16)
}

fn u32_property(node: &Node<'_>, name: &str) -> Option<u32> {
    node.get_prop(name)
        .map(|property| property.deserialize::<u32>())
}

fn machine_interrupt_files(
    node: &Node<'_>,
    controllers: &[CpuInterruptController],
) -> runtime::Result<Vec<MachineInterruptFile>> {
    let cells = u32_cells(node, "interrupts-extended").ok_or(runtime::Error::InvalidArgs)?;
    let mut interrupts = cells.chunks_exact(2);
    let mut machine_files = Vec::new();

    for (file_index, interrupt) in interrupts.by_ref().enumerate() {
        let phandle = interrupt[0];
        if interrupt[1] != MACHINE_EXTERNAL_INTERRUPT_ID {
            continue;
        }
        let controller = controllers
            .iter()
            .find(|controller| controller.phandle == phandle)
            .ok_or(runtime::Error::InvalidArgs)?;
        let file_index = u32::try_from(file_index).map_err(|_| runtime::Error::InvalidArgs)?;
        machine_files.push(MachineInterruptFile {
            hart_id: controller.hart_id,
            file_index,
        });
    }
    if !interrupts.remainder().is_empty() {
        return Err(runtime::Error::InvalidArgs);
    }
    Ok(machine_files)
}

fn u32_cells(node: &Node<'_>, name: &str) -> Option<Vec<u32>> {
    let bytes = node.get_prop(name)?.deserialize::<&[u8]>();
    let mut cells = Vec::new();
    let mut chunks = bytes.chunks_exact(size_of::<u32>());
    for chunk in &mut chunks {
        cells.push(u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }
    chunks.remainder().is_empty().then_some(cells)
}

fn validate_topology(
    guest_index_bits: u32,
    hart_index_bits: u32,
    group_index_bits: u32,
    group_index_shift: u32,
    file_page_shift: u32,
) -> runtime::Result<()> {
    if guest_index_bits > MAX_GUEST_INDEX_BITS
        || hart_index_bits > MAX_HART_INDEX_BITS
        || group_index_bits > MAX_GROUP_INDEX_BITS
        || group_index_shift > MAX_GROUP_INDEX_SHIFT
    {
        return Err(runtime::Error::InvalidArgs);
    }

    let hart_index_shift = file_page_shift
        .checked_add(guest_index_bits)
        .ok_or(runtime::Error::Overflow)?;
    let hart_index_end = hart_index_shift
        .checked_add(hart_index_bits)
        .ok_or(runtime::Error::Overflow)?;
    let group_index_end = group_index_shift
        .checked_add(group_index_bits)
        .ok_or(runtime::Error::Overflow)?;
    if hart_index_end > usize::BITS
        || (group_index_bits != 0
            && (group_index_shift < hart_index_end || group_index_end > usize::BITS))
    {
        return Err(runtime::Error::InvalidArgs);
    }
    Ok(())
}

fn map_hart_files(
    layout: &ImsicAddressLayout,
    register_ranges: &[DeviceRegisterRange],
    machine_files: &[MachineInterruptFile],
    enabled_harts: &HartEnableList,
    group_index_bits: u32,
) -> runtime::Result<[Option<DeviceRegisterRange>; NUM_HART_MAX]> {
    let topology_bits = layout.hart_index_bits + group_index_bits;
    let max_file_count = 1u64 << topology_bits;
    let hart_index_mask = low_bit_mask(layout.hart_index_bits);
    let group_index_mask = low_bit_mask(group_index_bits);
    let mut hart_files = [None; NUM_HART_MAX];

    for machine_file in machine_files {
        if machine_file.hart_id >= NUM_HART_MAX
            || u64::from(machine_file.file_index) >= max_file_count
        {
            return Err(runtime::Error::InvalidArgs);
        }
        let hart_index = machine_file.file_index & hart_index_mask;
        let group_index = (machine_file.file_index >> layout.hart_index_bits) & group_index_mask;
        let address = layout
            .machine_file_address(hart_index, group_index)
            .ok_or(runtime::Error::Overflow)?;
        let file = PhysAddrRange::from_start_len(address, driver::IMSIC_FILE_SPAN)?;
        let register_range = register_ranges
            .iter()
            .copied()
            .find(|register_range| register_range.contains(file))
            .ok_or(runtime::Error::InvalidArgs)?;
        let offset = file.start().as_usize() - register_range.start().as_usize();
        hart_files[machine_file.hart_id] =
            Some(register_range.subrange(offset, driver::IMSIC_FILE_SPAN)?);
    }

    if enabled_harts
        .iter()
        .enumerate()
        .any(|(hart_id, enabled)| *enabled && hart_files[hart_id].is_none())
    {
        return Err(runtime::Error::InvalidArgs);
    }
    Ok(hart_files)
}

const fn low_bit_mask(bits: u32) -> u32 {
    if bits == 0 { 0 } else { (1u32 << bits) - 1 }
}
