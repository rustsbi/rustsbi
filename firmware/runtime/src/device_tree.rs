//! Access to the device tree received at firmware entry.
//!
//! Memory nodes, reservations, and `status` values follow the Devicetree
//! Specification v0.4, sections 2.3.4, 3.4, and 5.3:
//! <https://github.com/devicetree-org/devicetree-specification/releases/tag/v0.4>.

use alloc::vec::Vec;
use core::mem::size_of;

use fdt::{Fdt, node::FdtNode};

use crate::memory::{
    DeviceRegisterRange, MemoryRegistry, PhysAddr, PhysAddrRange, SupervisorMemory,
    locate_firmware_image,
};
use crate::spacemit_k1::SpacemitK1Registers;
use crate::{Error, Result};

mod patch;

// Devicetree Specification v0.4, section 5.2: the structure header is ten
// 32-bit big-endian fields, with `totalsize` as its second field, and the
// blob is aligned to an 8-byte boundary.
const FDT_HEADER_SIZE: usize = 10 * size_of::<u32>();
const FDT_TOTAL_SIZE_OFFSET: usize = size_of::<u32>();
const FDT_ALIGNMENT: usize = 8;

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
/// Construction validates the complete FDT at the firmware-entry trust seam.
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
    fdt: Fdt<'tree>,
    fdt_storage: PhysAddrRange,
}

impl<'tree> PlatformView<'tree> {
    /// Returns the parsed device tree used for platform discovery.
    pub const fn fdt(&self) -> &Fdt<'tree> {
        &self.fdt
    }

    /// Resolves an absolute path or alias and returns it only when every node
    /// on the path is enabled.
    pub fn find_enabled_node(&self, path: &str) -> Option<FdtNode<'_, 'tree>> {
        let resolved_path = if path.starts_with('/') {
            path
        } else {
            let aliases = self.fdt.find_node("/aliases")?;
            if !node_is_enabled(aliases) {
                return None;
            }
            aliases.property(path)?.as_str()?
        };

        let mut current = self.fdt.find_node("/")?;
        if !node_is_enabled(current) {
            return None;
        }
        if resolved_path == "/" {
            return Some(current);
        }

        for name in resolved_path.strip_prefix('/')?.split('/') {
            if name.is_empty() {
                return None;
            }
            current = current.children().find(|child| {
                child.name == name
                    || (!name.contains('@') && child.name.split('@').next() == Some(name))
            })?;
            if !node_is_enabled(current) {
                return None;
            }
        }
        Some(current)
    }

    /// Returns the root node for read-only platform discovery.
    pub fn root(&self) -> FdtNode<'_, 'tree> {
        self.fdt
            .find_node("/")
            .expect("a validated FDT always contains its root node")
    }

    /// Returns the register ranges of an enabled node from this description.
    pub fn device_registers(
        &self,
        node: FdtNode<'_, 'tree>,
    ) -> Result<Option<Vec<DeviceRegisterRange>>> {
        if !node_is_enabled(node) {
            return Ok(None);
        }
        let Some(property) = node.property("reg") else {
            return Ok(None);
        };

        let encoded = PhysAddrRange::from_start_len(
            PhysAddr::new(property.value.as_ptr() as usize),
            property.value.len(),
        )?;
        if !self.fdt_storage.contains(encoded) {
            return Err(Error::AccessDenied);
        }

        let registers = node.reg().ok_or(Error::InvalidArgs)?;
        let mut ranges = Vec::new();
        for register in registers {
            let range = PhysAddrRange::from_start_len(
                PhysAddr::new(register.starting_address as usize),
                register.size.ok_or(Error::InvalidArgs)?,
            )?;
            ranges.push(DeviceRegisterRange::from_description(range));
        }
        Ok((!ranges.is_empty()).then_some(ranges))
    }

    /// Returns the first register range of an enabled node.
    ///
    /// Most device drivers consume only the primary `reg` entry. Callers such
    /// as IMSIC discovery use [`Self::device_registers`] when they need the
    /// complete layout.
    pub fn device_register(&self, node: FdtNode<'_, 'tree>) -> Result<Option<DeviceRegisterRange>> {
        if !node_is_enabled(node) {
            return Ok(None);
        }
        let Some(property) = node.property("reg") else {
            return Ok(None);
        };

        let encoded = PhysAddrRange::from_start_len(
            PhysAddr::new(property.value.as_ptr() as usize),
            property.value.len(),
        )?;
        if !self.fdt_storage.contains(encoded) {
            return Err(Error::AccessDenied);
        }

        let mut registers = node.reg().ok_or(Error::InvalidArgs)?;
        let register = registers.next().ok_or(Error::InvalidArgs)?;
        let range = PhysAddrRange::from_start_len(
            PhysAddr::new(register.starting_address as usize),
            register.size.ok_or(Error::InvalidArgs)?,
        )?;
        Ok(Some(DeviceRegisterRange::from_description(range)))
    }

    /// Returns K1 fixed-register capabilities when this description identifies K1.
    pub fn spacemit_k1_registers(&self) -> Result<Option<SpacemitK1Registers>> {
        SpacemitK1Registers::from_root(self.root())
    }

    /// Returns a SoC capability when the root node identifies `S`.
    pub fn soc<S: crate::soc::Soc>(&self) -> Result<Option<S>> {
        S::from_root(self.root())
    }

    /// Returns the RTC V203 GPRCM window from the BSP's fixed four-cell property.
    /// Unlike `reg`, `gprcm_reg` contains an absolute 64-bit address and size.
    pub fn sunxi_rtc_v203_gprcm(
        &self,
        node: FdtNode<'_, 'tree>,
    ) -> Result<Option<DeviceRegisterRange>> {
        crate::sunxi_rtc_v203::gprcm_registers(node, self.fdt_storage)
    }
}

impl PlatformDescription {
    /// Builds a description from an entry-point FDT address.
    ///
    /// # Safety
    ///
    /// `address` must be non-null and point to readable memory containing the
    /// fixed FDT header and the complete FDT declared by its `totalsize`
    /// field. The bytes must remain readable and unchanged for as long as the
    /// returned description is used.
    unsafe fn from_raw(address: PhysAddr) -> Result<Self> {
        // SAFETY: the caller of this function provides the pointer validity
        // and lifetime guarantees documented above.
        let source = unsafe { fdt_source(address)? };
        patch::validate(source)?;
        let fdt = Fdt::new(source).map_err(|_| Error::InvalidArgs)?;
        fdt.find_node("/").ok_or(Error::InvalidArgs)?;
        Ok(Self { address })
    }

    fn source(&self) -> Result<&[u8]> {
        // SAFETY: construction validated the complete FDT, and the
        // description's ownership contract keeps its storage readable and
        // unchanged.
        unsafe { fdt_source(self.address) }
    }

    /// Inspects the device tree through a temporary root-node view.
    ///
    /// The view cannot escape `inspect`. This lets policy select drivers
    /// without acquiring raw access to the FDT storage. Validation failures
    /// from the inspection closure are returned directly.
    pub fn inspect<R>(
        &self,
        inspect: impl for<'tree> FnOnce(PlatformView<'tree>) -> Result<R>,
    ) -> Result<R> {
        let source = self.source()?;
        let fdt = Fdt::new(source).map_err(|_| Error::InvalidArgs)?;
        let fdt_storage = PhysAddrRange::from_start_len(self.address, source.len())?;
        inspect(PlatformView { fdt, fdt_storage })
    }

    /// Derives supervisor memory and the MMIO registry from the FDT.
    ///
    /// RAM and reserved ranges are read by Runtime. MMIO windows may then be
    /// acquired only from physical-address holes outside those ranges.
    pub fn memory_resources(&self) -> Result<(SupervisorMemory, MemoryRegistry)> {
        let (ram, reserved) = self.memory_ranges()?;
        MemoryRegistry::from_ranges(ram, reserved)
    }

    /// Prepares the device tree passed to the next stage.
    ///
    /// Adds the firmware reservation and hides selected nodes from the next
    /// stage. If no edits are requested, the original address is returned.
    ///
    /// `handoff_bank` is the RAM bank holding this firmware image; a rewritten
    /// tree is handed over inside that bank.
    pub fn prepare_next_stage(
        self,
        handoff_bank: PhysAddrRange,
        firmware_reservation: Option<PhysAddrRange>,
        hidden_node_paths: &[&str],
    ) -> Result<PhysAddr> {
        if firmware_reservation.is_none() && hidden_node_paths.is_empty() {
            return Ok(self.address);
        }

        let source = self.source()?;
        let reservation = firmware_reservation
            .map(|reservation| {
                let address =
                    u64::try_from(reservation.start().as_usize()).map_err(|_| Error::Overflow)?;
                let size = u64::try_from(reservation.size()).map_err(|_| Error::Overflow)?;
                patch::Reservation::new(address, size).ok_or(Error::InvalidArgs)
            })
            .transpose()?;
        let rewritten = patch::prepare_next_stage(source, reservation, hidden_node_paths)?;
        hand_over_in_ram_bank(handoff_bank, &rewritten)
    }

    fn memory_ranges(&self) -> Result<(Vec<PhysAddrRange>, Vec<PhysAddrRange>)> {
        let source = self.source()?;
        let fdt = Fdt::new(source).map_err(|_| Error::InvalidArgs)?;
        let mut ram = Vec::new();
        for node in fdt
            .all_nodes()
            .filter(|node| node.name.split('@').next() == Some("memory") && node_is_enabled(*node))
        {
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
            .filter(|node| node_is_enabled(*node))
        {
            for child in node.children().filter(|child| node_is_enabled(*child)) {
                if child.property("reg").is_none() {
                    continue;
                }
                let regions = child.reg().ok_or(Error::InvalidArgs)?;
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

/// Offset below the top of the RAM bank at which OpenSBI and RustSBI-QEMU hand
/// the device tree over; the rewritten tree is put in the same slot.
const HANDOFF_TREE_OFFSET: usize = 2 * 1024 * 1024;

/// Copies the next-stage tree into `bank` and returns its address.
///
/// The tree cannot be handed over from the firmware image: this very tree marks
/// the image `no-map`, so a kernel that enables paging before it reads the tree
/// leaves the image out of its boot mapping and then faults on the pointer it
/// was given. Handing the tree over at `HANDOFF_TREE_OFFSET` instead leaves it
/// in ordinary memory, outside both the image and the range the tree reserves.
fn hand_over_in_ram_bank(bank: PhysAddrRange, bytes: &[u8]) -> Result<PhysAddr> {
    let firmware_image = locate_firmware_image()?;
    let bank_end = bank.end().as_usize();
    // A tree too large for the conventional slot grows down from the top of the
    // bank instead of overrunning it.
    let start = if bytes.len() <= HANDOFF_TREE_OFFSET {
        bank_end
            .checked_sub(HANDOFF_TREE_OFFSET)
            .or_else(|| bank_end.checked_sub(bytes.len()))
    } else {
        bank_end.checked_sub(bytes.len())
    }
    .ok_or(Error::NotEnoughResources)?;
    // Rounding down rather than up keeps the end of the tree inside the bank;
    // `bytes` need not be a whole number of alignment units long.
    let start = start & !(FDT_ALIGNMENT - 1);
    // The tree must fit in the bank and start after the firmware image.
    if start < bank.start().as_usize() || start < firmware_image.end().as_usize() {
        return Err(Error::NotEnoughResources);
    }

    // SAFETY: the tree spans `[start, start + bytes.len())`. Subtracting from
    // the bank end keeps that span inside the bank, and the checks above keep
    // it clear of the firmware image this code runs from. `start` is 8-byte
    // aligned, and `bytes` is a heap allocation that lives inside that image,
    // so the two ranges cannot overlap.
    unsafe {
        core::ptr::copy_nonoverlapping(bytes.as_ptr(), start as *mut u8, bytes.len());
    }
    Ok(PhysAddr::new(start))
}

/// Returns the complete FDT byte range described by an entry-point address.
///
/// # Safety
///
/// The caller must ensure that `address` is non-null and that the fixed FDT
/// header plus the memory declared by its `totalsize` field are readable and
/// unchanged for the returned slice's lifetime.
unsafe fn fdt_source<'a>(address: PhysAddr) -> Result<&'a [u8]> {
    if address.as_usize() == 0 {
        return Err(Error::InvalidArgs);
    }
    // SAFETY: the caller guarantees that the address points to a readable FDT
    // header and complete declared span.
    let fdt = unsafe { Fdt::from_ptr(address.as_usize() as *const u8) }
        .map_err(|_| Error::InvalidArgs)?;
    // SAFETY: the same caller guarantee covers the complete `totalsize` span.
    Ok(unsafe { core::slice::from_raw_parts(address.as_usize() as *const u8, fdt.total_size()) })
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
pub fn node_is_enabled(node: FdtNode<'_, '_>) -> bool {
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
    use super::*;

    // Keep the trust-seam checks together: the fixture is shared because the
    // production invariant is about the FDT trust boundary, not test-tree data.
    #[test]
    fn from_raw_rejects_fdt_without_root_node() {
        let mut storage = alloc::vec![0u64; 16];
        let source = minimal_fdt(&mut storage, 0);
        write_test_u32(source, 56, 9);

        // SAFETY: `minimal_fdt` built a complete, writable fixture whose
        // declared span remains live in `storage` for this call.
        let result =
            unsafe { PlatformDescription::from_raw(PhysAddr::new(source.as_ptr() as usize)) };
        assert_eq!(result.err(), Some(Error::InvalidArgs));
    }

    #[test]
    fn from_raw_accepts_a_four_byte_aligned_fdt() {
        let mut storage = alloc::vec![0u64; 16];
        let source = minimal_fdt(&mut storage, 4);

        // SAFETY: `minimal_fdt` built a complete fixture whose declared span
        // remains live in `storage` for this call.
        unsafe { PlatformDescription::from_raw(PhysAddr::new(source.as_ptr() as usize)) }
            .expect("a 4-byte-aligned FDT is valid");
    }

    #[test]
    fn status_accepts_enabled_values_and_defaults_to_enabled() {
        assert!(status_is_enabled(None));
        assert!(status_is_enabled(Some(b"ok\0")));
        assert!(status_is_enabled(Some(b"okay\0")));
        assert!(!status_is_enabled(Some(b"disabled\0")));
    }

    fn write_test_u32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
    }

    fn minimal_fdt(storage: &mut [u64], offset: usize) -> &mut [u8] {
        // SAFETY: the fixture uses an in-bounds byte range within the live
        // `u64` allocation; the four-byte offset models an unaligned FDT.
        let source = unsafe {
            core::slice::from_raw_parts_mut(storage.as_mut_ptr().cast::<u8>().add(offset), 72)
        };
        source.fill(0);
        write_test_u32(source, 0, 0xd00d_feed);
        write_test_u32(source, 4, 72);
        write_test_u32(source, 8, 56);
        write_test_u32(source, 12, 72);
        write_test_u32(source, 16, 40);
        write_test_u32(source, 20, 17);
        write_test_u32(source, 24, 16);
        write_test_u32(source, 36, 16);
        write_test_u32(source, 56, 1);
        write_test_u32(source, 64, 2);
        write_test_u32(source, 68, 9);
        source
    }
}
