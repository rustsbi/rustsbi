//! Discovery of machine-level IMSIC interrupt files.
//!
//! [RISC-V AIA 1.0], section 3.6, defines address layout and interrupt-file semantics.
//! The pinned [IMSIC binding] defines property bounds and defaults.
//!
//! [RISC-V AIA 1.0]: https://docs.riscv.org/reference/aia/_attachments/riscv-interrupts.pdf
//! [IMSIC binding]: https://github.com/torvalds/linux/blob/a500db7819c50db59e55f1b4fa1c3baa5a2616f3/Documentation/devicetree/bindings/interrupt-controller/riscv%2Cimsics.yaml

use alloc::vec::Vec;
use core::mem::size_of;
use runtime::{
    Error, FdtNode, PlatformView, Result,
    hart::HartId,
    ipi::Iid,
    memory::{PhysAddrRange, RegRange},
    node_is_enabled,
};

use super::{AddressLayout, Desc, IMSIC_FILE_SPAN};
use crate::devicetree::{is_cpu_node, u32_property};

const MACHINE_EXTERNAL_INTERRUPT_ID: u32 = 11;
const MAX_GUEST_INDEX_BITS: u32 = 7;
const MAX_HART_INDEX_BITS: u32 = 15;
const MAX_GROUP_INDEX_BITS: u32 = 7;
const MAX_GROUP_INDEX_SHIFT: u32 = 55;
const DEFAULT_GROUP_INDEX_SHIFT: u32 = 24;
const FIRMWARE_IPI_IDENTITY: u16 = 1;

/// A CPU interrupt-controller phandle and its owning hart.
#[derive(Clone, Copy)]
struct CpuInterruptController {
    phandle: u32,
    hart_id: usize,
}

/// A machine-external interrupt file selected from `interrupts-extended`.
struct MachineInterruptFile {
    hart_id: usize,
    file_index: u32,
}

/// Associates an enabled CPU interrupt-controller node with its owning hart.
///
/// The caller supplies the hart ID, including for CPU nodes that are disabled.
fn cpu_interrupt_controller(
    node: FdtNode<'_, '_>,
    hart_id: usize,
) -> Option<CpuInterruptController> {
    if node.name.split('@').next() != Some("interrupt-controller")
        || !is_cpu_interrupt_controller(node)
    {
        return None;
    }
    Some(CpuInterruptController {
        phandle: phandle(node)?,
        hart_id,
    })
}

/// Parses one machine IMSIC node into a description.
pub(super) fn describe(
    platform: &PlatformView<'_>,
    node: FdtNode<'_, '_>,
    register_ranges: &[RegRange],
) -> Result<Option<Desc>> {
    let first_register_range = register_ranges.first().ok_or(Error::InvalidArgs)?;
    if register_ranges
        .iter()
        .any(|range| !range.has_aligned_bounds(IMSIC_FILE_SPAN))
    {
        return Err(Error::InvalidArgs);
    }

    let machine_base = first_register_range.start();
    let num_iids = interrupt_identity_count(node)?;
    let controllers = cpu_interrupt_controllers(platform)?;
    let machine_files = machine_interrupt_files(node, &controllers)?;
    if machine_files.is_empty() {
        debug!(
            "IMSIC: node at 0x{:x} is not wired to MachineExternal, skipping",
            machine_base.as_usize()
        );
        return Ok(None);
    }

    // Derive the default hart-index width from every `interrupts-extended`
    // entry: disabled CPUs may still own physical interrupt-file slots.
    let file_count = node
        .property("interrupts-extended")
        .ok_or(Error::InvalidArgs)?
        .value
        .len()
        / (2 * size_of::<u32>());
    let file_count = u32::try_from(file_count).map_err(|_| Error::InvalidArgs)?;
    let default_hart_index_bits = file_count
        .checked_sub(1)
        .map_or(0, |last| u32::BITS - last.leading_zeros());
    let hart_index_bits =
        u32_property(node, "riscv,hart-index-bits").unwrap_or(default_hart_index_bits);
    let group_index_bits = u32_property(node, "riscv,group-index-bits").unwrap_or(0);
    let group_index_shift =
        u32_property(node, "riscv,group-index-shift").unwrap_or(DEFAULT_GROUP_INDEX_SHIFT);
    let guest_index_bits = u32_property(node, "riscv,guest-index-bits").unwrap_or(0);
    let file_page_shift = IMSIC_FILE_SPAN.trailing_zeros();
    validate_topology(
        guest_index_bits,
        hart_index_bits,
        group_index_bits,
        group_index_shift,
        file_page_shift,
    )?;

    let ipi_iid =
        Iid::new(FIRMWARE_IPI_IDENTITY).expect("BUG: firmware IPI identity must be nonzero");
    let layout = AddressLayout::new(
        machine_base,
        hart_index_bits,
        group_index_shift,
        file_page_shift + guest_index_bits,
    );
    let hart_files = map_hart_files(&layout, register_ranges, &machine_files, group_index_bits)?;

    Ok(Some(Desc {
        layout,
        ipi_iid,
        num_iids,
        hart_files,
    }))
}

fn cpu_interrupt_controllers(platform: &PlatformView<'_>) -> Result<Vec<CpuInterruptController>> {
    let cpus = platform
        .find_enabled_node("/cpus")
        .ok_or(Error::InvalidArgs)?;
    let mut controllers = Vec::new();
    for cpu in cpus.children().filter(|node| is_cpu_node(*node)) {
        let hart_id = PlatformView::hart_id(cpu)?;
        // Disabled CPUs still occupy positions in `interrupts-extended`.
        controllers.extend(
            cpu.children()
                .filter_map(|node| cpu_interrupt_controller(node, hart_id)),
        );
    }
    Ok(controllers)
}

/// Returns whether a node is an enabled `riscv,cpu-intc` controller.
fn is_cpu_interrupt_controller(node: FdtNode<'_, '_>) -> bool {
    node_is_enabled(node)
        && node.compatible().is_some_and(|compatibles| {
            compatibles
                .all()
                .any(|device_id| device_id == "riscv,cpu-intc")
        })
}

/// Returns the node's `phandle`, falling back to `linux,phandle`.
fn phandle(node: FdtNode<'_, '_>) -> Option<u32> {
    u32_property(node, "phandle").or_else(|| u32_property(node, "linux,phandle"))
}

/// Reads the identity count from `riscv,num-ids` as a `u16`.
fn interrupt_identity_count(node: FdtNode<'_, '_>) -> Result<u16> {
    u32_property(node, "riscv,num-ids")
        .and_then(|num_iids| u16::try_from(num_iids).ok())
        .ok_or(Error::InvalidArgs)
}

/// Collects `interrupts-extended` entries that route Machine External interrupts.
fn machine_interrupt_files(
    node: FdtNode<'_, '_>,
    controllers: &[CpuInterruptController],
) -> Result<Vec<MachineInterruptFile>> {
    let cells = u32_cells(node, "interrupts-extended").ok_or(Error::InvalidArgs)?;
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
            .ok_or(Error::InvalidArgs)?;
        if HartId::from_raw(controller.hart_id).is_err() {
            continue;
        }
        let file_index = u32::try_from(file_index).map_err(|_| Error::InvalidArgs)?;
        machine_files.push(MachineInterruptFile {
            hart_id: controller.hart_id,
            file_index,
        });
    }
    if !interrupts.remainder().is_empty() {
        return Err(Error::InvalidArgs);
    }
    Ok(machine_files)
}

/// Reads a property as big-endian 32-bit cells.
fn u32_cells(node: FdtNode<'_, '_>, name: &str) -> Option<Vec<u32>> {
    let bytes = node.property(name)?.value;
    let mut cells = Vec::new();
    let mut chunks = bytes.chunks_exact(size_of::<u32>());
    for chunk in &mut chunks {
        cells.push(u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }
    chunks.remainder().is_empty().then_some(cells)
}

/// Validates the IMSIC index widths and their bit-field placement.
fn validate_topology(
    guest_index_bits: u32,
    hart_index_bits: u32,
    group_index_bits: u32,
    group_index_shift: u32,
    file_page_shift: u32,
) -> Result<()> {
    if guest_index_bits > MAX_GUEST_INDEX_BITS
        || hart_index_bits > MAX_HART_INDEX_BITS
        || group_index_bits > MAX_GROUP_INDEX_BITS
        || group_index_shift > MAX_GROUP_INDEX_SHIFT
    {
        return Err(Error::InvalidArgs);
    }

    let hart_index_shift = file_page_shift
        .checked_add(guest_index_bits)
        .ok_or(Error::Overflow)?;
    let hart_index_end = hart_index_shift
        .checked_add(hart_index_bits)
        .ok_or(Error::Overflow)?;
    let group_index_end = group_index_shift
        .checked_add(group_index_bits)
        .ok_or(Error::Overflow)?;
    if hart_index_end > usize::BITS
        || (group_index_bits != 0
            && (group_index_shift < hart_index_end || group_index_end > usize::BITS))
    {
        return Err(Error::InvalidArgs);
    }
    Ok(())
}

/// Maps machine interrupt files into the register ranges that contain them.
fn map_hart_files(
    layout: &AddressLayout,
    register_ranges: &[RegRange],
    machine_files: &[MachineInterruptFile],
    group_index_bits: u32,
) -> Result<Vec<RegRange>> {
    let topology_bits = layout.hart_index_bits + group_index_bits;
    let max_file_count = 1u64 << topology_bits;
    let hart_index_mask = low_bit_mask(layout.hart_index_bits);
    let group_index_mask = low_bit_mask(group_index_bits);
    let mut hart_files = alloc::vec![None; HartId::count()];

    for machine_file in machine_files {
        let hart = HartId::from_raw(machine_file.hart_id).map_err(|_| Error::InvalidArgs)?;
        if u64::from(machine_file.file_index) >= max_file_count
            || hart_files[hart.index()].is_some()
        {
            return Err(Error::InvalidArgs);
        }
        let hart_index = machine_file.file_index & hart_index_mask;
        let group_index = (machine_file.file_index >> layout.hart_index_bits) & group_index_mask;
        let address = layout
            .machine_file_address(hart_index, group_index)
            .ok_or(Error::Overflow)?;
        let file = PhysAddrRange::from_start_len(address, IMSIC_FILE_SPAN)?;
        let register_range = register_ranges
            .iter()
            .copied()
            .find(|register_range| register_range.contains(file))
            .ok_or(Error::InvalidArgs)?;
        let offset = file.start().as_usize() - register_range.start().as_usize();
        hart_files[hart.index()] = Some(register_range.subrange(offset, IMSIC_FILE_SPAN)?);
    }

    hart_files
        .into_iter()
        .map(|file| file.ok_or(Error::InvalidArgs))
        .collect()
}

/// Returns a mask with the low `bits` bits set.
const fn low_bit_mask(bits: u32) -> u32 {
    if bits == 0 { 0 } else { (1u32 << bits) - 1 }
}
