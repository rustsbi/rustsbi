//! Validated machine-level IMSIC binding for firmware work notification.

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ops::Range;

use dtoolkit::fdt::Fdt;

use crate::boot::BootInfo;
use crate::boot::device_tree::{
    BindingError, compatible, cpu_interrupt_controllers, enabled, exact_node, model,
    optional_u32_property, reg_ranges, u32_cells, u32_property,
};
use crate::hart::{IpiDevice, IpiError, Notification};

mod arch;

use arch::{claim_current_file, current_hart_id, device_fence, initialize_current_file};

const QEMU_MODEL: &str = "riscv-virtio,qemu";
const QEMU_MACHINE_BASE: usize = 0x2400_0000;
const MACHINE_EXTERNAL_INTERRUPT: u32 = 11;
const FIRMWARE_IPI_IID: u16 = 1;
const IMSIC_COMPATIBLE: [&str; 2] = ["riscv,imsics", "riscv,imsic"];
const INTERRUPT_FILE_SIZE: usize = 0x1000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ImsicError {
    Binding(BindingError),
    Unauthorized,
    InvalidTopology,
    Hardware,
}

/// Validated address and identity layout of one machine-level IMSIC instance.
pub(super) struct ImsicLayout {
    pub(super) register_ranges: Vec<Range<usize>>,
    hart_files: Vec<HartInterruptFile>,
    interrupt_identity_count: u16,
    notification_identity: u16,
    pub(super) hart_index_width: u32,
}

/// Machine interrupt file assigned to one physical hart.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct HartInterruptFile {
    hart_id: usize,
    address: usize,
}

impl ImsicLayout {
    pub(super) fn from_dtb(boot: &BootInfo, path: &str) -> Result<Self, ImsicError> {
        let fdt = Fdt::new(boot.dtb().as_bytes())
            .map_err(|_| ImsicError::Binding(BindingError::DeviceTree))?;
        let node = exact_node(&fdt, path).map_err(ImsicError::Binding)?;
        if !enabled(&node) || !compatible(&node, &IMSIC_COMPATIBLE) {
            return Err(ImsicError::Binding(BindingError::Unsupported));
        }
        let ranges = reg_ranges(node).map_err(ImsicError::Binding)?;
        if ranges.iter().any(|range| {
            !range.start.is_multiple_of(INTERRUPT_FILE_SIZE)
                || !(range.end - range.start).is_multiple_of(INTERRUPT_FILE_SIZE)
        }) {
            return Err(ImsicError::InvalidTopology);
        }
        if model(&fdt) != QEMU_MODEL
            || ranges.first().map(|range| range.start) != Some(QEMU_MACHINE_BASE)
        {
            return Err(ImsicError::Unauthorized);
        }

        let num_ids = u32_property(&node, "riscv,num-ids").map_err(ImsicError::Binding)?;
        let num_ids = u16::try_from(num_ids).map_err(|_| ImsicError::InvalidTopology)?;
        if num_ids <= FIRMWARE_IPI_IID || num_ids > 2048 {
            return Err(ImsicError::InvalidTopology);
        }
        let intc_harts = cpu_interrupt_controllers(&fdt).map_err(ImsicError::Binding)?;
        let interrupt_cells =
            u32_cells(&node, "interrupts-extended").map_err(ImsicError::Binding)?;
        let mut entries = interrupt_cells.chunks_exact(2);
        let mut raw_files = Vec::new();
        for (file_index, entry) in entries.by_ref().enumerate() {
            if entry[1] != MACHINE_EXTERNAL_INTERRUPT {
                continue;
            }
            let hart_id = intc_harts
                .iter()
                .find_map(|(phandle, hart_id)| (*phandle == entry[0]).then_some(*hart_id))
                .ok_or(ImsicError::InvalidTopology)?;
            let file_index = u32::try_from(file_index).map_err(|_| ImsicError::InvalidTopology)?;
            raw_files.push((hart_id, file_index));
        }
        if !entries.remainder().is_empty() || raw_files.len() != intc_harts.len() {
            return Err(ImsicError::InvalidTopology);
        }

        let default_hart_bits = topology_bits(raw_files.len())?;
        let hart_index_bits =
            optional_u32_property(&node, "riscv,hart-index-bits", default_hart_bits)
                .map_err(ImsicError::Binding)?;
        let group_index_bits = optional_u32_property(&node, "riscv,group-index-bits", 0)
            .map_err(ImsicError::Binding)?;
        let group_index_shift = optional_u32_property(&node, "riscv,group-index-shift", 24)
            .map_err(ImsicError::Binding)?;
        validate_topology(hart_index_bits, group_index_bits, group_index_shift)?;

        let mut hart_files: Vec<HartInterruptFile> = Vec::new();
        for (hart_id, file_index) in raw_files {
            let address = file_address(
                QEMU_MACHINE_BASE,
                file_index,
                hart_index_bits,
                group_index_bits,
                group_index_shift,
            )?;
            let end = address
                .checked_add(INTERRUPT_FILE_SIZE)
                .ok_or(ImsicError::InvalidTopology)?;
            if !ranges
                .iter()
                .any(|range| address >= range.start && end <= range.end)
                || hart_files.iter().any(|file| file.address == address)
                || hart_files.iter().any(|file| file.hart_id == hart_id)
            {
                return Err(ImsicError::InvalidTopology);
            }
            hart_files.push(HartInterruptFile { hart_id, address });
        }

        Ok(Self {
            register_ranges: ranges,
            hart_files,
            interrupt_identity_count: num_ids,
            notification_identity: FIRMWARE_IPI_IID,
            hart_index_width: hart_index_bits,
        })
    }

    pub(super) fn into_device(self) -> (Arc<dyn IpiDevice>, Vec<usize>) {
        let harts = self.hart_files.iter().map(|file| file.hart_id).collect();
        let device: Arc<dyn IpiDevice> = Arc::new(Imsic {
            hart_files: self.hart_files,
            interrupt_identity_count: self.interrupt_identity_count,
            notification_identity: self.notification_identity,
        });
        (device, harts)
    }

    pub(super) fn hart_ids(&self) -> Vec<usize> {
        self.hart_files.iter().map(|file| file.hart_id).collect()
    }
}

struct Imsic {
    hart_files: Vec<HartInterruptFile>,
    interrupt_identity_count: u16,
    notification_identity: u16,
}

impl IpiDevice for Imsic {
    fn prepare_current_hart(&self) -> Result<(), IpiError> {
        let hart_id = current_hart_id();
        if !self.hart_files.iter().any(|file| file.hart_id == hart_id) {
            return Err(IpiError::InvalidHart);
        }
        initialize_current_file(self.interrupt_identity_count, self.notification_identity)
            .map_err(|_| IpiError::Failed)
    }

    fn notify(&self, hart_id: usize) {
        let Some(address) = self
            .hart_files
            .iter()
            .find_map(|file| (file.hart_id == hart_id).then_some(file.address))
        else {
            return;
        };
        device_fence();
        // SAFETY: construction proved that this aligned page belongs to the
        // selected hart's machine interrupt file and that the IID is valid.
        unsafe {
            (address as *mut u32).write_volatile(u32::from(self.notification_identity).to_le());
        }
        device_fence();
    }

    fn claim(&self, hart_id: usize) {
        claim_current_file(hart_id)
    }

    fn notification(&self) -> Notification {
        Notification::External
    }
}

fn topology_bits(count: usize) -> Result<u32, ImsicError> {
    let count = u32::try_from(count).map_err(|_| ImsicError::InvalidTopology)?;
    Ok(if count <= 1 {
        0
    } else {
        u32::BITS - (count - 1).leading_zeros()
    })
}

fn validate_topology(hart_bits: u32, group_bits: u32, group_shift: u32) -> Result<(), ImsicError> {
    if hart_bits >= usize::BITS
        || group_bits >= usize::BITS
        || hart_bits
            .checked_add(group_bits)
            .is_none_or(|bits| bits >= usize::BITS)
        || group_shift < 12u32.saturating_add(hart_bits)
        || group_shift >= usize::BITS
    {
        Err(ImsicError::InvalidTopology)
    } else {
        Ok(())
    }
}

fn file_address(
    base: usize,
    file_index: u32,
    hart_bits: u32,
    group_bits: u32,
    group_shift: u32,
) -> Result<usize, ImsicError> {
    let topology_bits = hart_bits
        .checked_add(group_bits)
        .ok_or(ImsicError::InvalidTopology)?;
    let capacity = 1usize
        .checked_shl(topology_bits)
        .ok_or(ImsicError::InvalidTopology)?;
    let file_index = usize::try_from(file_index).map_err(|_| ImsicError::InvalidTopology)?;
    if file_index >= capacity {
        return Err(ImsicError::InvalidTopology);
    }
    let hart_mask = 1usize
        .checked_shl(hart_bits)
        .ok_or(ImsicError::InvalidTopology)?
        .wrapping_sub(1);
    let hart_offset = (file_index & hart_mask)
        .checked_shl(12)
        .ok_or(ImsicError::InvalidTopology)?;
    let group_offset = (file_index >> hart_bits)
        .checked_shl(group_shift)
        .ok_or(ImsicError::InvalidTopology)?;
    base.checked_add(hart_offset)
        .and_then(|address| address.checked_add(group_offset))
        .ok_or(ImsicError::InvalidTopology)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qemu_file_arithmetic_supports_sparse_harts_without_using_them_as_indices() {
        assert_eq!(
            file_address(QEMU_MACHINE_BASE, 0, 2, 0, 24),
            Ok(0x2400_0000)
        );
        assert_eq!(
            file_address(QEMU_MACHINE_BASE, 3, 2, 0, 24),
            Ok(0x2400_3000)
        );
        assert_eq!(
            file_address(QEMU_MACHINE_BASE, 4, 2, 0, 24),
            Err(ImsicError::InvalidTopology)
        );
    }

    #[test]
    fn group_and_hart_offsets_do_not_share_address_bits() {
        assert_eq!(
            file_address(QEMU_MACHINE_BASE, 5, 2, 1, 24),
            Ok(QEMU_MACHINE_BASE + (1 << 12) + (1 << 24))
        );
        assert_eq!(
            validate_topology(13, 0, 24),
            Err(ImsicError::InvalidTopology)
        );
    }
}
