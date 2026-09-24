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
    /// The rewritten tree is placed at a next-stage-friendly address (see
    /// [`Self::place_rewritten`]); `firmware_image_range` supplies the range
    /// the placement must keep clear, and is passed independently of
    /// `firmware_reservation` because an existing platform reservation makes
    /// the reservation argument `None` while the image still occupies RAM.
    pub fn prepare_next_stage(
        self,
        firmware_reservation: Option<PhysAddrRange>,
        hidden_node_paths: &[&str],
        firmware_image_range: Option<PhysAddrRange>,
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
        Ok(place_rewritten(&self, rewritten, firmware_image_range))
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

fn leak_aligned(bytes: Vec<u8>) -> PhysAddr {
    // The next stage keeps this rewritten DTB after `PlatformDescription` is
    // consumed, so intentionally leak the aligned allocation to preserve its
    // lifetime. Native-endian words only provide alignment; the bytes remain
    // in their original order when copied into the allocation.
    let mut words = Vec::with_capacity(bytes.len().div_ceil(size_of::<u64>()));
    for chunk in bytes.chunks(size_of::<u64>()) {
        let mut encoded = [0; size_of::<u64>()];
        encoded[..chunk.len()].copy_from_slice(chunk);
        words.push(u64::from_ne_bytes(encoded));
    }
    PhysAddr::new(words.leak().as_ptr() as usize)
}

/// Offset of the next-stage FDT copy within the firmware's RAM bank.
///
/// Upstream OpenSBI hands its FDT to the next stage 34 MiB above the start of
/// RAM (0x82200000 on QEMU virt), far from the firmware image. Next-stage
/// loaders derive their own load addresses from the FDT location — the Hermit
/// loader, for example, places the kernel just above the two-megabyte-aligned
/// span containing the tree — so a heap allocation next to this firmware's
/// image would pin the next stage directly on top of the loader.
const NEXT_STAGE_FDT_BANK_OFFSET: usize = 0x0220_0000;

const FDT_BANK_ALIGNMENT: usize = 0x20_0000;

/// Copies the rewritten tree to a checked next-stage-friendly address, or
/// keeps the heap allocation when no such address fits.
fn place_rewritten(
    description: &PlatformDescription,
    rewritten: Vec<u8>,
    firmware_image_range: Option<PhysAddrRange>,
) -> PhysAddr {
    let placement = next_stage_layout(description, &rewritten)
        .and_then(|layout| checked_placement(&layout, firmware_image_range));
    match placement {
        Some(address) => {
            // SAFETY: `checked_placement` verified that the span lies inside a
            // RAM bank handed to the next stage and clear of the firmware
            // image, the incoming FDT, the initrd, and every existing
            // reservation. The heap is a fixed `.bss` array inside the
            // firmware image, so no allocation can reach this span, and this
            // firmware never touches it again afterwards: the address is only
            // handed to the next stage through the boot ABI.
            unsafe {
                core::ptr::copy_nonoverlapping(
                    rewritten.as_ptr(),
                    address as *mut u8,
                    rewritten.len(),
                );
            }
            PhysAddr::new(address)
        }
        None => leak_aligned(rewritten),
    }
}

/// Describes every region a next-stage FDT placement must keep clear of.
struct NextStageLayout {
    ram: Vec<PhysAddrRange>,
    reserved: Vec<PhysAddrRange>,
    fdt_storage: Option<PhysAddrRange>,
    initrd: Option<PhysAddrRange>,
    len: usize,
}

/// Gathers the placement inputs for the current description.
///
/// Returns `None` when the incoming tree cannot be inspected; the caller then
/// falls back to the heap allocation.
fn next_stage_layout(
    description: &PlatformDescription,
    rewritten: &[u8],
) -> Option<NextStageLayout> {
    let source = description.source().ok()?;
    let (ram, reserved) = description.memory_ranges().ok()?;
    let fdt_storage = PhysAddrRange::from_start_len(description.address, source.len()).ok()?;
    let initrd = initrd_range(source);
    Some(NextStageLayout {
        ram,
        reserved,
        fdt_storage: Some(fdt_storage),
        initrd,
        len: rewritten.len(),
    })
}

fn initrd_range(source: &[u8]) -> Option<PhysAddrRange> {
    let fdt = Fdt::new(source).ok()?;
    let chosen = fdt.find_node("/chosen")?;
    let start = chosen.property("linux,initrd-start")?.as_usize()?;
    let end = chosen.property("linux,initrd-end")?.as_usize()?;
    PhysAddrRange::from_start_len(PhysAddr::new(start), end.checked_sub(start)?).ok()
}

/// Returns a two-megabyte-aligned candidate address for a next-stage FDT of
/// `layout.len` bytes, or `None` when no checked placement fits.
///
/// The candidate sits `NEXT_STAGE_FDT_BANK_OFFSET` above the start of the RAM
/// bank containing the firmware image, must fit inside that bank, and must
/// not overlap the firmware image, the incoming FDT, the initrd, or any
/// existing memory reservation.
fn checked_placement(
    layout: &NextStageLayout,
    firmware_image_range: Option<PhysAddrRange>,
) -> Option<usize> {
    let bank = firmware_image_range
        .as_ref()
        .and_then(|image| layout.ram.iter().find(|bank| bank.contains(*image)))
        .or_else(|| layout.ram.first())?;

    let unaligned = bank
        .start()
        .as_usize()
        .checked_add(NEXT_STAGE_FDT_BANK_OFFSET)?;
    let address = unaligned
        .div_ceil(FDT_BANK_ALIGNMENT)
        .checked_mul(FDT_BANK_ALIGNMENT)?;
    let candidate = PhysAddrRange::from_start_len(PhysAddr::new(address), layout.len).ok()?;
    if !bank.contains(candidate) {
        return None;
    }

    let overlaps = |range: &PhysAddrRange| {
        range.start().as_usize() < address + layout.len && address < range.end().as_usize()
    };
    if firmware_image_range
        .iter()
        .chain(layout.fdt_storage.iter())
        .chain(layout.initrd.iter())
        .chain(layout.reserved.iter())
        .any(overlaps)
    {
        return None;
    }
    Some(address)
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
    use alloc::vec;

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

    // The QEMU virt shape: firmware at the bank start, one bank, no
    // reservations. The candidate must mirror upstream OpenSBI's 0x82200000.
    #[test]
    fn placement_matches_opensbi_qemu_virt() {
        let layout = NextStageLayout {
            ram: vec![ram(0x8000_0000, 0x0800_0000)],
            reserved: vec![],
            fdt_storage: Some(range(0x8006_8000, 0x2000)),
            initrd: Some(range(0x8420_0000, 0x10_4000)),
            len: 0x2000,
        };
        let firmware = range(0x8000_0000, 0x7_0000);

        assert_eq!(
            checked_placement(&layout, Some(firmware)),
            Some(0x8220_0000),
        );
    }

    #[test]
    fn placement_rejects_small_bank() {
        let layout = NextStageLayout {
            ram: vec![ram(0x8000_0000, 0x0100_0000)],
            reserved: vec![],
            fdt_storage: Some(range(0x8006_8000, 0x2000)),
            initrd: None,
            len: 0x2000,
        };
        let firmware = range(0x8000_0000, 0x7_0000);

        assert_eq!(checked_placement(&layout, Some(firmware)), None);
    }

    #[test]
    fn placement_rejects_overlap_with_initrd() {
        let layout = NextStageLayout {
            ram: vec![ram(0x8000_0000, 0x0800_0000)],
            reserved: vec![],
            fdt_storage: Some(range(0x8006_8000, 0x2000)),
            // The initrd covers the default candidate region.
            initrd: Some(range(0x8220_0000, 0x100_0000)),
            len: 0x2000,
        };
        let firmware = range(0x8000_0000, 0x7_0000);

        assert_eq!(checked_placement(&layout, Some(firmware)), None);
    }

    #[test]
    fn placement_rejects_overlap_with_reservation() {
        let layout = NextStageLayout {
            ram: vec![ram(0x8000_0000, 0x0800_0000)],
            reserved: vec![range(0x8220_0000, 0x1_0000)],
            fdt_storage: Some(range(0x8006_8000, 0x2000)),
            initrd: None,
            len: 0x2000,
        };
        let firmware = range(0x8000_0000, 0x7_0000);

        assert_eq!(checked_placement(&layout, Some(firmware)), None);
    }

    // Without a known firmware image the first bank still anchors the
    // candidate instead of giving up.
    #[test]
    fn placement_falls_back_to_first_bank() {
        let layout = NextStageLayout {
            ram: vec![ram(0x8000_0000, 0x0800_0000)],
            reserved: vec![],
            fdt_storage: Some(range(0x8006_8000, 0x2000)),
            initrd: None,
            len: 0x2000,
        };

        assert_eq!(checked_placement(&layout, None), Some(0x8220_0000));
    }

    fn range(start: usize, len: usize) -> PhysAddrRange {
        PhysAddrRange::from_start_len(PhysAddr::new(start), len).unwrap()
    }

    fn ram(start: usize, len: usize) -> PhysAddrRange {
        range(start, len)
    }
}
