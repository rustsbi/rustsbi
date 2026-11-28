//! Device-tree acquisition and memory resources tied to the linked firmware.

use super::{
    DeviceTreeHandoff, FDT_HEADER_SIZE, FDT_TOTAL_SIZE_OFFSET, PlatformDescription, PlatformView,
};
use crate::boot::linked_image_range;
use crate::memory::{PhysAddr, PhysAddrRange};
use crate::{Error, Result};

impl DeviceTreeHandoff {
    /// Claims the selected boot device tree.
    ///
    /// The selected address may be the entry argument itself or an FDT linked
    /// inside the current firmware image.
    /// An FDT containing interior NOPs requires an initialized allocator.
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
            // SAFETY: The caller supplies a readable, immutable FDT at the selected address.
            unsafe { validate_linked_fdt(selected_address)? };
        }
        // SAFETY: The caller establishes an immutable source, a trusted memory
        // map, and exclusive destination storage through handoff.
        unsafe { PlatformDescription::from_raw(selected_address, destination) }
    }
}

impl PlatformView<'_> {
    /// Returns the SoC description selected by the root node's compatible strings.
    pub fn soc<S: crate::soc::Soc>(&self) -> Result<Option<S>> {
        S::from_root(self.root())
    }
}

/// Checks that the FDT header and its declared extent lie inside the linked image.
///
/// # Safety
///
/// The header at `address` must be readable, initialized and stable during this call.
unsafe fn validate_linked_fdt(address: PhysAddr) -> Result<()> {
    let firmware = linked_image_range()?;
    let header = PhysAddrRange::from_start_len(address, FDT_HEADER_SIZE)?;
    if !firmware.contains(header) {
        return Err(Error::AccessDenied);
    }

    // SAFETY: The caller supplies a readable header containing the totalsize field.
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
