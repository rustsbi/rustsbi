//! Access to the device tree received at firmware entry.
//!
//! Memory nodes, reservations, and `status` values follow the Devicetree
//! Specification v0.4, sections 2.3.4, 3.4, and 5.3:
//! <https://github.com/devicetree-org/devicetree-specification/releases/tag/v0.4>.

use alloc::vec::Vec;
use core::mem::size_of;

use fdt::{Fdt, node::FdtNode};
use spin::Once;

use crate::memory::{
    DeviceRegisterRange, MemoryRegistry, PhysAddr, PhysAddrRange, SupervisorMemory,
    locate_firmware_image,
};
use crate::spacemit_k1::SpacemitK1Registers;
use crate::{Error, Result};

mod patch;

// Devicetree Specification v0.4, section 5.2: the structure header is ten
// 32-bit big-endian fields, with `totalsize` as its second field.
const FDT_HEADER_SIZE: usize = 10 * size_of::<u32>();
const FDT_TOTAL_SIZE_OFFSET: usize = size_of::<u32>();
const RUSTSBI_FDT_PADDING: usize = 1024;

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

    /// Describes the incoming DTB buffer under RustSBI's padding contract.
    ///
    /// # Safety
    ///
    /// The entry address must point to a readable FDT. The loader must provide
    /// exclusive writable storage covering its initial `totalsize` plus 1024
    /// bytes, live until the next stage finishes using it. This method checks
    /// address arithmetic; it cannot establish the allocation's actual capacity.
    pub unsafe fn incoming_destination(&self) -> Result<PhysAddrRange> {
        // SAFETY: the caller guarantees the incoming FDT's readable span.
        let size = unsafe { fdt_source(self.address)? }.len();
        let capacity = size
            .checked_add(RUSTSBI_FDT_PADDING)
            .ok_or(Error::Overflow)?;
        PhysAddrRange::from_start_len(self.address, capacity)
    }

    /// Claims the selected boot device tree.
    ///
    /// The selected address may be the entry argument itself or an FDT linked
    /// inside the current firmware image.
    ///
    /// # Safety
    ///
    /// The selected FDT must remain readable and unchanged until this description
    /// is consumed. `destination` must describe exclusively writable storage,
    /// live until the next stage finishes using it. It may overlap the selected
    /// FDT, but must not overlap other live objects. No external access is allowed
    /// while this description owns the destination. A range alone does not prove
    /// that its backing storage exists.
    pub unsafe fn claim(
        self,
        selected_address: PhysAddr,
        destination: PhysAddrRange,
    ) -> Result<PlatformDescription> {
        if selected_address == self.address {
            // SAFETY: entry and the caller guarantee the source and destination
            // storage contracts, including their lifetimes and ownership.
            return unsafe { PlatformDescription::from_raw(selected_address, destination) };
        }

        validate_linked_fdt(selected_address)?;
        // SAFETY: `validate_linked_fdt` checked both the FDT header and its
        // declared complete span against the linked firmware image. The caller
        // guarantees the source lifetime and exclusive destination storage.
        unsafe { PlatformDescription::from_raw(selected_address, destination) }
    }
}

/// The platform description supplied by the previous stage.
///
/// Construction validates the complete FDT at the firmware-entry trust seam.
/// The value is not clonable, so only its owner can create temporary tree
/// views or derive physical-memory access from it.
pub struct PlatformDescription {
    address: PhysAddr,
    destination: PhysAddrRange,
    memory_ranges: Once<(Vec<PhysAddrRange>, Vec<PhysAddrRange>)>,
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
    /// Iterates over hardware IDs of enabled CPU nodes without allocating.
    pub fn hart_ids(&self) -> Result<impl Iterator<Item = Result<usize>> + '_> {
        let cpus = self.find_enabled_node("/cpus").ok_or(Error::InvalidArgs)?;
        let cells = cpus
            .property("#address-cells")
            .and_then(|property| property.as_usize())
            .ok_or(Error::InvalidArgs)?;
        if !(1..=2).contains(&cells) {
            return Err(Error::InvalidArgs);
        }
        Ok(cpus
            .children()
            .filter(|node| node.name.split('@').next() == Some("cpu") && node_is_enabled(*node))
            .map(Self::hart_id))
    }

    /// Decodes the hardware ID of a CPU node, including disabled nodes.
    pub fn hart_id(node: FdtNode<'_, 'tree>) -> Result<usize> {
        let reg = node.property("reg").ok_or(Error::InvalidArgs)?.value;
        let register = node
            .raw_reg()
            .and_then(|mut registers| registers.next())
            .ok_or(Error::InvalidArgs)?;
        if !matches!(register.address.len(), 4 | 8)
            || reg.len() != register.address.len()
            || !register.size.is_empty()
        {
            return Err(Error::InvalidArgs);
        }
        let raw = register
            .address
            .iter()
            .fold(0u64, |raw, byte| (raw << 8) | u64::from(*byte));
        usize::try_from(raw).map_err(|_| Error::Overflow)
    }

    pub(crate) fn storage_range(&self) -> PhysAddrRange {
        self.fdt_storage
    }

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

    /// Returns the `reg` entry named by the node's `reg-names` property.
    ///
    /// Returns `Ok(None)` when the node is disabled or carries no `reg-names`
    /// property; callers use that to apply a legacy positional fallback of
    /// their own. When `reg-names` is present, the requested name must exist
    /// and must have a corresponding `reg` entry: a node that names its
    /// resources has declared their meaning, so an absent name is an error
    /// rather than a cue to guess an index. A `reg-names` value that is not
    /// NUL-terminated is rejected as malformed.
    pub fn device_register_by_name(
        &self,
        node: FdtNode<'_, 'tree>,
        name: &str,
    ) -> Result<Option<DeviceRegisterRange>> {
        if !node_is_enabled(node) {
            return Ok(None);
        }
        let Some(names) = node.property("reg-names") else {
            return Ok(None);
        };

        let encoded = PhysAddrRange::from_start_len(
            PhysAddr::new(names.value.as_ptr() as usize),
            names.value.len(),
        )?;
        if !self.fdt_storage.contains(encoded) {
            return Err(Error::AccessDenied);
        }

        // Devicetree Specification v0.4, section 2.3.3: a string list is a
        // sequence of NUL-terminated strings, and an empty string is still a
        // list element. Positions therefore stay aligned with `reg` indices
        // even when a name in between is empty.
        if !names.value.ends_with(&[0]) {
            return Err(Error::InvalidArgs);
        }
        let index = names
            .value
            .split(|byte| *byte == 0)
            .position(|entry| !entry.is_empty() && entry == name.as_bytes())
            .ok_or(Error::InvalidArgs)?;

        let registers = self.device_registers(node)?.ok_or(Error::InvalidArgs)?;
        registers
            .get(index)
            .copied()
            .map(Some)
            .ok_or(Error::InvalidArgs)
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
    /// `address` must point to a readable FDT, unchanged until this description
    /// is consumed. `destination` must have exclusive writable backing storage
    /// that remains live until the next stage finishes using it. Source and
    /// destination may overlap; all source views end before any write occurs.
    /// The destination must not overlap any other live allocation or object.
    unsafe fn from_raw(address: PhysAddr, destination: PhysAddrRange) -> Result<Self> {
        if destination.start().as_usize() == 0 || destination.size() < FDT_HEADER_SIZE {
            return Err(Error::InvalidArgs);
        }
        u32::try_from(destination.size()).map_err(|_| Error::Overflow)?;
        // SAFETY: the caller of this function provides the pointer validity
        // and lifetime guarantees documented above.
        let source = unsafe { fdt_source(address)? };
        patch::validate(source)?;
        let fdt = Fdt::new(source).map_err(|_| Error::InvalidArgs)?;
        fdt.find_node("/").ok_or(Error::InvalidArgs)?;
        Ok(Self {
            address,
            destination,
            memory_ranges: Once::new(),
        })
    }

    pub(crate) fn validate_destination(
        &self,
        firmware: PhysAddrRange,
        next_stage_entry: usize,
        handoff: Option<PhysAddrRange>,
        ram: impl Iterator<Item = PhysAddrRange>,
    ) -> Result<()> {
        validate_destination(self.destination, firmware, next_stage_entry, handoff, ram)
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
        // SAFETY: construction validated the complete FDT. Ownership keeps
        // its storage readable and unchanged, and the view cannot escape.
        let fdt = unsafe { Fdt::from_ptr(self.address.as_usize() as *const u8) }
            .map_err(|_| Error::InvalidArgs)?;
        let fdt_storage = PhysAddrRange::from_start_len(self.address, fdt.total_size())?;
        inspect(PlatformView { fdt, fdt_storage })
    }

    /// Derives supervisor memory and the MMIO registry from the FDT.
    ///
    /// RAM and reserved ranges are read by Runtime. MMIO windows may then be
    /// acquired only from physical-address holes outside those ranges.
    pub fn memory_resources(&self) -> Result<(SupervisorMemory, MemoryRegistry)> {
        // The FDT is immutable, but the firmware span can grow during boot.
        // Reuse its ranges and derive access against the current span each time.
        let (ram, reserved) = self
            .memory_ranges
            .try_call_once(|| self.read_memory_ranges())?;
        MemoryRegistry::from_ranges(ram.iter().copied(), reserved.iter().copied())
    }

    /// Prepares the device tree passed to the next stage.
    ///
    /// Adds the firmware reservation and hides selected nodes, then copies the
    /// result into the destination supplied at construction. Even without edits,
    /// a distinct destination receives the source tree. `totalsize` includes the
    /// destination capacity and unused space is zeroed. Insufficient capacity
    /// returns an error before any destination bytes are written.
    pub fn prepare_next_stage(
        self,
        firmware_reservation: Option<PhysAddrRange>,
        hidden_node_paths: &[&str],
    ) -> Result<PhysAddr> {
        let destination = self.destination.start();
        let capacity = self.destination.size();
        let reservation = firmware_reservation
            .map(|reservation| {
                let address =
                    u64::try_from(reservation.start().as_usize()).map_err(|_| Error::Overflow)?;
                let size = u64::try_from(reservation.size()).map_err(|_| Error::Overflow)?;
                patch::Reservation::new(address, size).ok_or(Error::InvalidArgs)
            })
            .transpose()?;
        let mut rewritten = {
            let source = self.source()?;
            patch::prepare_next_stage(source, reservation, hidden_node_paths)?
        };
        if rewritten.len() > capacity {
            return Err(Error::NotEnoughResources);
        }
        // Publish the extra capacity with initialized padding for later edits.
        patch::set_total_size(&mut rewritten, capacity)?;
        // SAFETY: construction guarantees exclusive writable storage covering
        // `capacity` bytes. All tree views have ended, and consuming `self`
        // prevents further inspection. The checked output is a separate buffer.
        unsafe {
            core::ptr::copy_nonoverlapping(
                rewritten.as_ptr(),
                destination.as_usize() as *mut u8,
                rewritten.len(),
            );
            core::ptr::write_bytes(
                (destination.as_usize() as *mut u8).add(rewritten.len()),
                0,
                capacity - rewritten.len(),
            );
        }
        Ok(destination)
    }

    fn read_memory_ranges(&self) -> Result<(Vec<PhysAddrRange>, Vec<PhysAddrRange>)> {
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

// Check the entire destination before any secondary stack is used. The loader
// must still exclude the full next-stage image and other live objects; an entry
// address cannot describe those complete allocations.
fn validate_destination(
    destination: PhysAddrRange,
    firmware: PhysAddrRange,
    next_stage_entry: usize,
    handoff: Option<PhysAddrRange>,
    mut ram: impl Iterator<Item = PhysAddrRange>,
) -> Result<()> {
    if firmware.overlaps(destination)
        || (destination.start().as_usize()..destination.end().as_usize())
            .contains(&next_stage_entry)
        || handoff.is_some_and(|range| destination.overlaps(range))
    {
        return Err(Error::AccessDenied);
    }
    if !ram.any(|range| range.contains(destination)) {
        return Err(Error::NotEnoughResources);
    }
    Ok(())
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
        let mut storage = alloc::vec![0u64; 160];
        let source = minimal_fdt(&mut storage, 0);
        write_test_u32(source, 56, 9);

        // SAFETY: `minimal_fdt` built a complete, writable fixture whose
        // declared span remains live in `storage` for this call.
        let address = PhysAddr::new(source.as_mut_ptr() as usize);
        let destination = PhysAddrRange::from_start_len(address, source.len()).unwrap();
        let result = unsafe { PlatformDescription::from_raw(address, destination) };
        assert_eq!(result.err(), Some(Error::InvalidArgs));
    }

    #[test]
    fn from_raw_accepts_a_four_byte_aligned_fdt() {
        let mut storage = alloc::vec![0u64; 160];
        let source = minimal_fdt(&mut storage, 4);

        // SAFETY: `minimal_fdt` built a complete fixture whose declared span
        // remains live in `storage` for this call.
        let address = PhysAddr::new(source.as_mut_ptr() as usize);
        let destination = PhysAddrRange::from_start_len(address, source.len()).unwrap();
        unsafe { PlatformDescription::from_raw(address, destination) }
            .expect("a 4-byte-aligned FDT is valid");
    }

    #[test]
    fn copies_to_a_distinct_destination_with_and_without_edits() {
        for edit in [false, true] {
            let mut source_storage = [0u64; 9];
            minimal_fdt(&mut source_storage, 0);
            let original = source_storage;
            let source_address = PhysAddr::new(source_storage.as_mut_ptr() as usize);
            let mut target_storage = [0xa5a5_a5a5_a5a5_a5a5u64; 162];
            // The destination is larger than the source plus the default padding.
            let destination = test_destination(&mut target_storage, 8, 1280);
            // SAFETY: both allocations remain live; source is unchanged and the
            // description alone accesses the writable destination until consumed.
            let description =
                unsafe { PlatformDescription::from_raw(source_address, destination).unwrap() };
            let reservation = edit.then(|| {
                PhysAddrRange::from_start_len(PhysAddr::new(0x8000_0000), 0x1000).unwrap()
            });
            assert_eq!(
                description.prepare_next_stage(reservation, &[]).unwrap(),
                destination.start()
            );
            let tree = Fdt::new(&test_bytes(&target_storage)[8..1288]).unwrap();
            assert_eq!(tree.total_size(), 1280);
            assert_eq!(tree.find_node("/reserved-memory").is_some(), edit);
            assert_eq!(source_storage, original);
            assert_eq!(target_storage[0], 0xa5a5_a5a5_a5a5_a5a5);
            assert_eq!(target_storage[161], 0xa5a5_a5a5_a5a5_a5a5);
            assert_eq!(target_storage[160], 0);
        }
    }

    #[test]
    fn insufficient_destination_capacity_leaves_all_bytes_unchanged() {
        for edit in [false, true] {
            let mut source_storage = [0u64; 9];
            minimal_fdt(&mut source_storage, 0);
            let original_source = source_storage;
            let source_address = PhysAddr::new(source_storage.as_mut_ptr() as usize);
            let mut target_storage = [0xa5a5_a5a5_a5a5_a5a5u64; 16];
            let original_target = target_storage;
            let destination = test_destination(&mut target_storage, 8, 64);
            // SAFETY: the source and the 64-byte destination have separate live
            // backing allocations, exclusively accessed here until consumption.
            let description =
                unsafe { PlatformDescription::from_raw(source_address, destination).unwrap() };
            let reservation = edit.then(|| {
                PhysAddrRange::from_start_len(PhysAddr::new(0x8000_0000), 0x1000).unwrap()
            });
            assert_eq!(
                description.prepare_next_stage(reservation, &[]),
                Err(Error::NotEnoughResources)
            );
            assert_eq!(target_storage, original_target);
            assert_eq!(source_storage, original_source);
        }
    }

    #[test]
    fn patches_the_incoming_buffer_in_place() {
        let mut storage = [0u64; 138];
        minimal_fdt(&mut storage, 0);
        let address = PhysAddr::new(storage.as_mut_ptr() as usize);
        let handoff = DeviceTreeHandoff { address };
        // SAFETY: storage covers the 72-byte tree plus 1024 writable bytes.
        let destination = unsafe { handoff.incoming_destination().unwrap() };
        assert_eq!(destination.size(), 72 + 1024);
        // SAFETY: storage is exclusively owned and remains live until the test
        // finishes reading the result. The source and destination intentionally overlap.
        let description = unsafe { handoff.claim(address, destination).unwrap() };
        let reservation =
            PhysAddrRange::from_start_len(PhysAddr::new(0x8000_0000), 0x1000).unwrap();
        assert_eq!(
            description.prepare_next_stage(Some(reservation), &[]),
            Ok(address)
        );
        let tree = Fdt::new(&test_bytes(&storage)[..destination.size()]).unwrap();
        assert!(
            tree.find_node("/reserved-memory/mmode_resv1@80000000")
                .is_some()
        );
        assert_eq!(tree.total_size(), destination.size());
    }

    #[test]
    fn unedited_tree_fits_an_exact_size_destination() {
        let mut source_storage = [0u64; 9];
        minimal_fdt(&mut source_storage, 0);
        let source_address = PhysAddr::new(source_storage.as_mut_ptr() as usize);
        let mut target_storage = [0u64; 9];
        let destination = test_destination(&mut target_storage, 0, 72);
        // SAFETY: source and destination are separate live 72-byte allocations.
        let description =
            unsafe { PlatformDescription::from_raw(source_address, destination).unwrap() };
        description.prepare_next_stage(None, &[]).unwrap();
        assert_eq!(source_storage, target_storage);
    }

    fn test_destination(storage: &mut [u64], offset: usize, capacity: usize) -> PhysAddrRange {
        assert!(offset + capacity <= core::mem::size_of_val(storage));
        let address = storage.as_mut_ptr().cast::<u8>().wrapping_add(offset) as usize;
        PhysAddrRange::from_start_len(PhysAddr::new(address), capacity).unwrap()
    }

    fn test_bytes(storage: &[u64]) -> &[u8] {
        // SAFETY: every byte of the initialized u64 slice can be read, and the
        // returned shared view borrows that same storage.
        unsafe {
            core::slice::from_raw_parts(
                storage.as_ptr().cast::<u8>(),
                core::mem::size_of_val(storage),
            )
        }
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
