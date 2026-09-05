//! Access to the device tree received at firmware entry.
//!
//! Memory nodes, reservations, and `status` values follow the Devicetree
//! Specification v0.4, sections 2.3.4, 3.4, and 5.3:
//! <https://github.com/devicetree-org/devicetree-specification/releases/tag/v0.4>.

use alloc::vec::Vec;
use core::mem::size_of;

use fdt::{Fdt, node::FdtNode};
use serde_device_tree::{Dtb, DtbPtr, buildin::Node};

use crate::memory::{
    DeviceRegisterRange, MemoryRegistry, PhysAddr, PhysAddrRange, SupervisorMemory,
    locate_firmware_image,
};
use crate::spacemit_k1::SpacemitK1Registers;
use crate::{Error, Result};

// Devicetree Specification v0.4, section 5.2: the structure header is ten
// 32-bit big-endian fields, with `totalsize` as its second field.
const FDT_HEADER_SIZE: usize = 10 * size_of::<u32>();
const FDT_TOTAL_SIZE_OFFSET: usize = size_of::<u32>();

/// The device-tree address received at the firmware entry point.
///
/// The generated entry bridge receives this opaque value directly from the
/// boot ABI. Its private representation prevents safe policy code from
/// manufacturing another entry capability.
#[doc(hidden)]
#[repr(transparent)]
pub struct DeviceTreeHandoff {
    address: PhysAddr,
}

impl DeviceTreeHandoff {
    /// Returns the address supplied by the previous stage.
    #[inline]
    pub const fn address(&self) -> PhysAddr {
        self.address
    }

    /// Claims the selected boot device tree.
    ///
    /// The selected address may be the entry argument itself or an FDT linked
    /// inside the current firmware image.
    pub fn claim(self, selected_address: PhysAddr) -> Result<PlatformDescription> {
        if selected_address == self.address {
            // SAFETY: values of this opaque type enter Rust only through the
            // generated firmware-entry ABI, whose contract covers the FDT.
            return unsafe { PlatformDescription::from_raw(selected_address) };
        }

        validate_linked_fdt(selected_address)?;
        // SAFETY: `validate_linked_fdt` checked both the FDT header and its
        // declared complete span against the linked firmware image.
        unsafe { PlatformDescription::from_raw(selected_address) }
    }
}

/// The platform description supplied by the previous stage.
///
/// Construction validates the FDT header at the firmware-entry trust seam.
/// The value is not clonable, so only its owner can create temporary tree
/// views or derive physical-memory access from it.
pub struct PlatformDescription {
    address: PhysAddr,
}

/// A temporary, provenance-preserving view of a [`PlatformDescription`].
///
/// The view may inspect raw nodes, but it issues device-register capabilities
/// only for properties stored in the Platform Description that created it.
pub struct PlatformView<'tree> {
    root: Node<'tree>,
    fdt_storage: PhysAddrRange,
}

impl<'tree> PlatformView<'tree> {
    /// Returns the root node for read-only platform discovery.
    pub const fn root(&self) -> &Node<'tree> {
        &self.root
    }

    /// Returns the register ranges of an enabled node from this description.
    pub fn device_registers(&self, node: &Node<'tree>) -> Result<Option<Vec<DeviceRegisterRange>>> {
        if !node_is_enabled(node) {
            return Ok(None);
        }
        let Some(property) = node.get_prop("reg") else {
            return Ok(None);
        };

        let encoded = property.deserialize::<&[u8]>();
        let encoded =
            PhysAddrRange::from_start_len(PhysAddr::new(encoded.as_ptr() as usize), encoded.len())?;
        if !self.fdt_storage.contains(encoded) {
            return Err(Error::AccessDenied);
        }

        let registers = property.deserialize::<serde_device_tree::buildin::Reg>();
        let mut ranges = Vec::new();
        for register in registers.iter() {
            let range = PhysAddrRange::new(
                PhysAddr::new(register.0.start),
                PhysAddr::new(register.0.end),
            )?;
            ranges.push(DeviceRegisterRange::from_description(range));
        }
        Ok((!ranges.is_empty()).then_some(ranges))
    }

    /// Returns K1 fixed-register capabilities when this description identifies K1.
    pub fn spacemit_k1_registers(&self) -> Result<Option<SpacemitK1Registers>> {
        SpacemitK1Registers::from_root(&self.root)
    }
}

impl PlatformDescription {
    unsafe fn from_raw(address: PhysAddr) -> Result<Self> {
        if address.as_usize() == 0 {
            return Err(Error::InvalidArgs);
        }
        DtbPtr::from_raw(address.as_usize() as *mut u8).map_err(|_| Error::InvalidArgs)?;
        // SAFETY: the caller provides the lifetime and access guarantees
        // required by `Fdt::from_ptr`; the returned parser is dropped before
        // this function returns.
        unsafe { Fdt::from_ptr(address.as_usize() as *const u8) }
            .map_err(|_| Error::InvalidArgs)?;
        Ok(Self { address })
    }

    /// Returns the physical address of the FDT.
    #[inline]
    pub const fn address(&self) -> PhysAddr {
        self.address
    }

    /// Inspects the device tree through a temporary root-node view.
    ///
    /// The view cannot escape `inspect`. This lets policy select drivers
    /// without acquiring raw access to the FDT storage. Validation failures
    /// from the inspection closure are returned directly.
    pub fn inspect<R>(
        &mut self,
        inspect: impl for<'tree> FnOnce(PlatformView<'tree>) -> Result<R>,
    ) -> Result<R> {
        // SAFETY: construction established that this complete FDT remains
        // readable while the Platform Description exists.
        let fdt = unsafe { Fdt::from_ptr(self.address.as_usize() as *const u8) }
            .map_err(|_| Error::InvalidArgs)?;
        let fdt_storage = PhysAddrRange::from_start_len(self.address, fdt.total_size())?;
        let dtb_pointer =
            DtbPtr::from_raw(self.address.as_usize() as *mut u8).map_err(|_| Error::InvalidArgs)?;
        let dtb = Dtb::from(dtb_pointer).share();
        let root = serde_device_tree::from_raw_mut(&dtb).map_err(|_| Error::InvalidArgs)?;
        inspect(PlatformView { root, fdt_storage })
    }

    /// Derives supervisor RAM and device-register access from this FDT.
    ///
    /// RAM and reserved ranges are read by Runtime. MMIO windows may then be
    /// acquired only from physical-address holes outside those ranges.
    pub fn into_memory_resources(self) -> Result<(SupervisorMemory, MemoryRegistry)> {
        let (ram, reserved) = self.memory_ranges()?;
        MemoryRegistry::from_ranges(ram, reserved)
    }

    fn memory_ranges(&self) -> Result<(Vec<PhysAddrRange>, Vec<PhysAddrRange>)> {
        // SAFETY: `PlatformDescription::from_raw` established that the FDT remains
        // readable and valid while this value exists.
        let fdt = unsafe { Fdt::from_ptr(self.address.as_usize() as *const u8) }
            .map_err(|_| Error::InvalidArgs)?;
        let mut ram = Vec::new();
        for node in fdt.all_nodes().filter(|node| {
            node.name.split('@').next() == Some("memory") && fdt_node_is_enabled(*node)
        }) {
            let regions = node.reg().ok_or(Error::InvalidArgs)?;
            for region in regions {
                record_nonempty_range(&mut ram, region.starting_address as usize, region.size)?;
            }
        }
        if ram.is_empty() {
            return Err(Error::NotEnoughResources);
        }

        let mut reserved = Vec::new();
        for reservation in fdt.memory_reservations() {
            record_nonempty_range(
                &mut reserved,
                reservation.address() as usize,
                Some(reservation.size()),
            )?;
        }
        if let Some(node) = fdt
            .find_node("/reserved-memory")
            .filter(|node| fdt_node_is_enabled(*node))
        {
            for child in node.children().filter(|child| fdt_node_is_enabled(*child)) {
                let Some(regions) = child.reg() else {
                    continue;
                };
                for region in regions {
                    record_nonempty_range(
                        &mut reserved,
                        region.starting_address as usize,
                        region.size,
                    )?;
                }
            }
        }
        Ok((ram, reserved))
    }
}

fn validate_linked_fdt(address: PhysAddr) -> Result<()> {
    let firmware = locate_firmware_image()?;
    let header = PhysAddrRange::from_start_len(address, FDT_HEADER_SIZE)?;
    if !firmware.contains(header) {
        return Err(Error::AccessDenied);
    }

    // SAFETY: the complete fixed-size header lies inside the linked image.
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

/// Returns whether a Platform Description node is available for use.
pub fn node_is_enabled(node: &Node<'_>) -> bool {
    status_is_enabled(
        node.get_prop("status")
            .map(|property| property.deserialize::<&[u8]>()),
    )
}

fn fdt_node_is_enabled(node: FdtNode<'_, '_>) -> bool {
    status_is_enabled(node.property("status").map(|status| status.value))
}

fn status_is_enabled(status: Option<&[u8]>) -> bool {
    let Some(status) = status else {
        return true;
    };
    matches!(status.strip_suffix(&[0]).unwrap_or(status), b"ok" | b"okay")
}

fn record_nonempty_range(
    ranges: &mut Vec<PhysAddrRange>,
    start: usize,
    size: Option<usize>,
) -> Result<()> {
    let size = size.ok_or(Error::InvalidArgs)?;
    if size != 0 {
        ranges.push(PhysAddrRange::from_start_len(PhysAddr::new(start), size)?);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::status_is_enabled;

    #[test]
    fn status_accepts_enabled_values_and_defaults_to_enabled() {
        assert!(status_is_enabled(None));
        assert!(status_is_enabled(Some(b"ok\0")));
        assert!(status_is_enabled(Some(b"okay\0")));
        assert!(!status_is_enabled(Some(b"disabled\0")));
    }
}
