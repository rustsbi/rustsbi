//! Device-tree acquisition and memory resources tied to the linked firmware.

use super::{
    DeviceTreeHandoff, FDT_HEADER_SIZE, FDT_TOTAL_SIZE_OFFSET, PlatformDescription, PlatformView,
};
use crate::boot::{linked_image_range, locate_firmware_image};
use crate::memory::{MemoryRegistry, PhysAddr, PhysAddrRange, SupervisorMemory};
use crate::{Error, Result};

impl DeviceTreeHandoff {
    /// Claims the selected boot device tree.
    ///
    /// The selected address may be the entry argument itself or an FDT linked
    /// inside the current firmware image.
    ///
    /// # Safety
    ///
    /// 1. Keep the selected FDT readable and unchanged until this description
    ///    is consumed. Its RAM, reserved memory and device ranges must match the
    ///    hardware because they authorize subsequent RAM and MMIO accesses.
    /// 2. Provide exclusively writable `destination` storage that remains live
    ///    until the next stage finishes using it. It may overlap the selected
    ///    FDT. Exclude other live objects and external accesses while the
    ///    description owns the destination.
    pub unsafe fn claim(
        self,
        selected_address: PhysAddr,
        destination: PhysAddrRange,
    ) -> Result<PlatformDescription> {
        if selected_address != self.address {
            validate_linked_fdt(selected_address)?;
        }
        // SAFETY: The caller establishes an immutable source, a trusted memory
        // map, and exclusive destination storage through handoff.
        unsafe { PlatformDescription::from_raw(selected_address, destination) }
    }
}

impl PlatformDescription {
    /// Derives supervisor memory and the MMIO registry from the FDT.
    ///
    /// Each call excludes the current firmware extent from supervisor RAM and
    /// creates a fresh MMIO registry. Overlap checks apply to acquisitions
    /// made through the same registry.
    pub fn memory_resources(&self) -> Result<(SupervisorMemory, MemoryRegistry)> {
        // The FDT is immutable, but the firmware span can grow during boot.
        // Reuse its ranges and derive access against the current span each time.
        let (ram, reserved) = self
            .memory_ranges
            .try_call_once(|| self.read_memory_ranges())?;
        MemoryRegistry::from_ranges_with_firmware(
            locate_firmware_image()?,
            ram.iter().copied(),
            reserved.iter().copied(),
        )
    }
}

impl PlatformView<'_> {
    /// Returns the SoC description selected by the root node's compatible strings.
    pub fn soc<S: crate::soc::Soc>(&self) -> Result<Option<S>> {
        S::from_root(self.root())
    }
}

fn validate_linked_fdt(address: PhysAddr) -> Result<()> {
    let firmware = linked_image_range()?;
    let header = PhysAddrRange::from_start_len(address, FDT_HEADER_SIZE)?;
    if !firmware.contains(header) {
        return Err(Error::AccessDenied);
    }

    // SAFETY: The checked header contains the totalsize field, and claim's
    // caller guarantees that the selected FDT is readable and immutable.
    let total_size = unsafe {
        let total_size_pointer = address
            .as_usize()
            .checked_add(FDT_TOTAL_SIZE_OFFSET)
            .ok_or(Error::Overflow)? as *const u32;
        u32::from_be(total_size_pointer.read_unaligned()) as usize
    };
    let complete_fdt = PhysAddrRange::from_start_len(address, total_size)?;
    if firmware.contains(complete_fdt) {
        Ok(())
    } else {
        Err(Error::AccessDenied)
    }
}
