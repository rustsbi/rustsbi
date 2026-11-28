//! Access to the device tree received at firmware entry.
//!
//! Memory nodes, reservations, and `status` values follow the Devicetree
//! Specification v0.4, sections 2.3.4, 3.4, and 5.3:
//! <https://github.com/devicetree-org/devicetree-specification/releases/tag/v0.4>.

use alloc::vec::Vec;
use core::mem::size_of;

use crate::memory::{MemoryRegistry, PhysAddr, PhysAddrRange, RegRange, SupervisorMemory};
use crate::{Error, Result};
use fdt::{Fdt, node::FdtNode};

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
mod firmware;
mod patch;
mod sunxi_rtc_v203;

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
}

/// The platform description supplied by the previous stage.
///
/// Construction validates the complete FDT at the firmware-entry trust seam.
/// The value is not clonable, so only its owner can create temporary tree
/// views or derive physical-memory access from it.
pub struct PlatformDescription {
    source: PhysAddrRange,
    parser_copy: Option<Vec<u8>>,
    destination: PhysAddrRange,
    memory_registry: Option<MemoryRegistry>,
    supervisor_memory: Option<SupervisorMemory>,
}

/// A temporary, provenance-preserving view of a [`PlatformDescription`].
///
/// The view may inspect raw nodes, but it issues device-register capabilities
/// only for properties stored in the Platform Description that created it.
pub struct PlatformView<'tree> {
    fdt: Fdt<'tree>,
    source: &'tree [u8],
}

type EnabledNodeVisitor<'a, 'view, 'tree> = &'a mut dyn FnMut(
    FdtNode<'view, 'tree>,
    Option<FdtNode<'view, 'tree>>,
    &[&'tree str],
) -> Result<()>;

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
        decode_register_value(register.address)
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

    /// Iterates over enabled nodes depth first without recursion.
    ///
    /// Calls `visitor` with each enabled node, its parent node (or `None` for `/`),
    /// and the path segments from `/` to that node. Disabled subtrees are skipped.
    pub fn try_for_each_enabled_node<'view>(
        &'view self,
        mut visitor: impl FnMut(
            FdtNode<'view, 'tree>,
            Option<FdtNode<'view, 'tree>>,
            &[&'tree str],
        ) -> Result<()>,
    ) -> Result<()> {
        self.try_for_each_enabled_node_inner(&mut visitor)
    }

    fn try_for_each_enabled_node_inner<'view>(
        &'view self,
        visitor: EnabledNodeVisitor<'_, 'view, 'tree>,
    ) -> Result<()> {
        let structure = patch::format::Layout::parse(self.source)?.structure;
        let mut offset = 0usize;
        let mut nodes = self.fdt.all_nodes();
        let mut ancestors: Vec<FdtNode<'view, 'tree>> = Vec::new();
        let mut path: Vec<&'tree str> = Vec::new();
        let mut depth = 0usize;
        let mut disabled_depth: Option<usize> = None;

        while offset < structure.len() {
            match patch::format::take_u32(structure, &mut offset)? {
                patch::format::NOP => {}
                patch::format::PROPERTY => {
                    let length = patch::format::take_u32(structure, &mut offset)? as usize;
                    let _name_offset = patch::format::take_u32(structure, &mut offset)?;
                    let _ = patch::format::take_aligned(structure, &mut offset, length)?;
                }
                patch::format::BEGIN_NODE => {
                    let raw_name = patch::format::take_c_string(structure, &mut offset)?;
                    let expected_name = if raw_name.is_empty() { "/" } else { raw_name };
                    let node = nodes.next().ok_or(Error::InvalidArgs)?;
                    if node.name != expected_name {
                        return Err(Error::InvalidArgs);
                    }
                    if disabled_depth.is_none() {
                        if node_is_enabled(node) {
                            let parent = ancestors.last().copied();
                            path.push(node.name);
                            visitor(node, parent, path.as_slice())?;
                            ancestors.push(node);
                        } else {
                            disabled_depth = Some(depth);
                        }
                    }
                    depth = depth.checked_add(1).ok_or(Error::Overflow)?;
                }
                patch::format::END_NODE => {
                    depth = depth.checked_sub(1).ok_or(Error::InvalidArgs)?;
                    if disabled_depth == Some(depth) {
                        disabled_depth = None;
                    } else if disabled_depth.is_none() {
                        ancestors.pop();
                        path.pop();
                    }
                }
                patch::format::END => break,
                _ => return Err(Error::InvalidArgs),
            }
        }
        if depth != 0 || disabled_depth.is_some() {
            return Err(Error::InvalidArgs);
        }
        Ok(())
    }

    /// Returns the root node for read-only platform discovery.
    pub fn root(&self) -> FdtNode<'_, 'tree> {
        self.fdt
            .find_node("/")
            .expect("a validated FDT always contains its root node")
    }

    /// Returns the register ranges of an enabled node from this description.
    pub fn device_registers(&self, node: FdtNode<'_, 'tree>) -> Result<Option<Vec<RegRange>>> {
        let Some(registers) = self.registers(node)? else {
            return Ok(None);
        };
        let ranges: Vec<_> = registers.collect::<Result<_>>()?;
        Ok((!ranges.is_empty()).then_some(ranges))
    }

    fn registers(
        &self,
        node: FdtNode<'_, 'tree>,
    ) -> Result<Option<impl Iterator<Item = Result<RegRange>> + 'tree>> {
        if !node_is_enabled(node) {
            return Ok(None);
        }
        let Some(property) = node.property("reg") else {
            return Ok(None);
        };

        self.check_property_storage(property.value)?;
        let registers = node.raw_reg().ok_or(Error::InvalidArgs)?.map(|register| {
            RegRange::from_start_len(
                PhysAddr::new(decode_register_value(register.address)?),
                decode_register_value(register.size)?,
            )
        });
        Ok(Some(registers))
    }

    /// Returns the `reg` entry named by the node's `reg-names` property.
    ///
    /// Returns `Ok(None)` when the node is disabled or carries no `reg-names`
    /// property, allowing callers to apply their own positional fallback.
    /// When `reg-names` is present, the requested name must exist and have a
    /// corresponding `reg` entry.
    /// An absent name is an error rather than a cue to guess an index.
    /// A `reg-names` value that is not NUL-terminated is rejected as malformed.
    pub fn device_register_by_name(
        &self,
        node: FdtNode<'_, 'tree>,
        name: &str,
    ) -> Result<Option<RegRange>> {
        if !node_is_enabled(node) {
            return Ok(None);
        }
        let Some(names) = node.property("reg-names") else {
            return Ok(None);
        };

        self.check_property_storage(names.value)?;

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
    /// Most device drivers consume only the primary `reg` entry.
    /// Callers such as IMSIC discovery use [`Self::device_registers`]
    /// when they need the complete layout.
    pub fn device_register(&self, node: FdtNode<'_, 'tree>) -> Result<Option<RegRange>> {
        let Some(mut registers) = self.registers(node)? else {
            return Ok(None);
        };
        registers.next().ok_or(Error::InvalidArgs)?.map(Some)
    }

    fn check_property_storage(&self, bytes: &[u8]) -> Result<()> {
        if bytes.is_empty() {
            return Err(Error::InvalidArgs);
        }
        let tree = self.source.as_ptr_range();
        let property = bytes.as_ptr_range();
        if tree.start <= property.start && property.end <= tree.end {
            Ok(())
        } else {
            Err(Error::AccessDenied)
        }
    }
}

impl PlatformDescription {
    /// Builds a description from an entry-point FDT address.
    ///
    /// Trees with NOP tokens require an initialized heap for the parser copy.
    ///
    /// # Safety
    ///
    /// `address` must point to a readable FDT, unchanged until this description
    /// is consumed. `destination` must have exclusive writable backing storage
    /// that remains live until the next stage finishes using it. Source and
    /// destination may overlap; all source views end before any write occurs.
    /// The destination must not overlap any other live allocation or object.
    /// For firmware use, the RAM, reservation and device ranges must match the
    /// actual hardware from which access will be granted.
    unsafe fn from_raw(address: PhysAddr, destination: PhysAddrRange) -> Result<Self> {
        if destination.start().as_usize() == 0 || destination.size() < FDT_HEADER_SIZE {
            return Err(Error::InvalidArgs);
        }
        u32::try_from(destination.size()).map_err(|_| Error::Overflow)?;
        // SAFETY: the caller of this function provides the pointer validity
        // and lifetime guarantees documented above.
        let source = unsafe { fdt_source(address)? };
        let parser_copy = patch::parser_copy_if_needed(source)?;
        let fdt =
            Fdt::new(parser_copy.as_deref().unwrap_or(source)).map_err(|_| Error::InvalidArgs)?;
        fdt.find_node("/").ok_or(Error::InvalidArgs)?;
        Ok(Self {
            source: PhysAddrRange::from_start_len(address, source.len())?,
            parser_copy,
            destination,
            memory_registry: None,
            supervisor_memory: None,
        })
    }

    pub(crate) fn prepare_boot_memory(
        &mut self,
        linked: PhysAddrRange,
        firmware: PhysAddrRange,
        next_stage_entry: usize,
        handoff: Option<PhysAddrRange>,
    ) -> Result<()> {
        let (ram, reserved) = self.read_memory_ranges()?;
        let mut registry = MemoryRegistry::new(linked, ram, reserved)?;
        validate_destination(
            self.destination,
            firmware,
            next_stage_entry,
            handoff,
            registry.ram_ranges(),
        )?;
        if linked != firmware {
            registry.extend_firmware_to(firmware.end())?;
        }
        self.supervisor_memory = Some(registry.supervisor_memory(self.mapped_rom()?)?);
        self.memory_registry = Some(registry);
        Ok(())
    }

    /// Takes the MMIO registry after boot has reserved the complete firmware image.
    /// Returns `None` before that reservation or after the registry has been taken.
    pub fn take_memory_registry(&mut self) -> Option<MemoryRegistry> {
        self.memory_registry.take()
    }

    pub(crate) fn storage_range(&self) -> PhysAddrRange {
        self.source
    }

    fn source(&self) -> &[u8] {
        // SAFETY: from_raw validated this exact span. The caller keeps it
        // readable and immutable until this description is consumed.
        unsafe {
            core::slice::from_raw_parts(
                self.source.start().as_usize() as *const u8,
                self.source.size(),
            )
        }
    }

    fn parser_source(&self) -> &[u8] {
        self.parser_copy.as_deref().unwrap_or_else(|| self.source())
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
        let source = self.parser_source();
        inspect(PlatformView {
            fdt: Fdt::new(source).map_err(|_| Error::InvalidArgs)?,
            source,
        })
    }

    /// Hands off the device tree and releases supervisor RAM for SBI services.
    ///
    /// The tree is written before the returned memory can be used to access its
    /// backing storage. Returns [`Error::NotReady`] before boot has reserved the firmware.
    pub fn prepare_next_stage(
        mut self,
        firmware_reservation: Option<PhysAddrRange>,
        hidden_node_paths: &[&str],
    ) -> Result<(PhysAddr, SupervisorMemory)> {
        let memory = self.supervisor_memory.take().ok_or(Error::NotReady)?;
        let address = self.write_handoff_tree(firmware_reservation, hidden_node_paths)?;
        Ok((address, memory))
    }

    /// Writes the device tree passed to the next stage.
    ///
    /// Adds the firmware reservation and hides selected nodes, then copies the
    /// result into the destination supplied at construction. Even without edits,
    /// a distinct destination receives the source tree. `totalsize` includes the
    /// destination capacity and unused space is zeroed. Insufficient capacity
    /// returns an error before any destination bytes are written.
    fn write_handoff_tree(
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
        let mut rewritten =
            patch::prepare_next_stage(self.source(), reservation, hidden_node_paths)?;
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

    fn mapped_rom(&self) -> Result<Vec<PhysAddrRange>> {
        self.inspect(|view| {
            let mut ranges = Vec::new();
            let mut physical_depth = 1;
            view.try_for_each_enabled_node(|node, _, path| {
                let depth = path.len() - 1;
                if depth == 0 || depth > physical_depth {
                    return Ok(());
                }
                // Children inherit CPU physical addresses only across an identity bus.
                let identity = node.property("ranges").is_some_and(|p| p.value.is_empty());
                physical_depth = depth + usize::from(identity);
                if !node.compatible().is_some_and(|list| {
                    list.all()
                        .any(|item| matches!(item, "cfi-flash" | "jedec-flash" | "mtd-rom"))
                }) {
                    return Ok(());
                }
                for reg in node.raw_reg().into_iter().flatten() {
                    if reg.address.is_empty() || reg.size.is_empty() {
                        break;
                    }
                    let (Ok(start), Ok(size)) = (
                        decode_register_value(reg.address),
                        decode_register_value(reg.size),
                    ) else {
                        continue;
                    };
                    if let Ok(range) = PhysAddrRange::from_start_len(PhysAddr::new(start), size) {
                        ranges.push(range);
                    }
                }
                Ok(())
            })?;
            Ok(ranges)
        })
    }

    fn read_memory_ranges(&self) -> Result<(Vec<PhysAddrRange>, Vec<PhysAddrRange>)> {
        self.read_memory_ranges_using(checked_pointer_width::<usize>)
    }

    fn read_memory_ranges_using(
        &self,
        convert_address: impl Fn(u64) -> Result<usize>,
    ) -> Result<(Vec<PhysAddrRange>, Vec<PhysAddrRange>)> {
        let fdt = Fdt::new(self.parser_source()).map_err(|_| Error::InvalidArgs)?;
        let mut ram = Vec::new();
        for node in fdt
            .find_node("/")
            .ok_or(Error::InvalidArgs)?
            .children()
            .filter(|node| node.name.split('@').next() == Some("memory") && node_is_enabled(*node))
        {
            record_memory_node(&mut ram, node, &convert_address)?;
        }
        if ram.is_empty() {
            return Err(Error::NotEnoughResources);
        }

        let mut reserved = Vec::new();
        // fdt 0.1 exposes reservations as native-width addresses and sizes.
        // Decode the original 64-bit entries before checking the hart's XLEN.
        for entry in patch::format::Layout::parse(self.source())?
            .reservations
            .chunks_exact(patch::format::RESERVATION_ENTRY_SIZE)
        {
            let (address, size) = entry.split_at(size_of::<u64>());
            record_nonempty_range(
                &mut reserved,
                decode_register_cells(address)?,
                decode_register_cells(size)?,
                &convert_address,
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
                record_memory_node(&mut reserved, child, &convert_address)?;
            }
        }
        Ok((ram, reserved))
    }
}

fn decode_register_value(bytes: &[u8]) -> Result<usize> {
    checked_pointer_width(decode_register_cells(bytes)?)
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
    // SAFETY: the same caller guarantee covers the complete declared span.
    Ok(unsafe { core::slice::from_raw_parts(address.as_usize() as *const u8, fdt.total_size()) })
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

fn record_memory_node(
    ranges: &mut Vec<PhysAddrRange>,
    node: FdtNode<'_, '_>,
    convert_address: &impl Fn(u64) -> Result<usize>,
) -> Result<()> {
    for reg in node.raw_reg().ok_or(Error::InvalidArgs)? {
        record_nonempty_range(
            ranges,
            decode_register_cells(reg.address)?,
            decode_register_cells(reg.size)?,
            convert_address,
        )?;
    }
    Ok(())
}

fn record_nonempty_range(
    ranges: &mut Vec<PhysAddrRange>,
    start: u64,
    size: u64,
    convert_address: &impl Fn(u64) -> Result<usize>,
) -> Result<()> {
    if size != 0 {
        let end = start.checked_add(size).ok_or(Error::Overflow)?;
        ranges.push(PhysAddrRange::new(
            PhysAddr::new(convert_address(start)?),
            PhysAddr::new(convert_address(end)?),
        )?);
    }
    Ok(())
}

fn decode_register_cells(bytes: &[u8]) -> Result<u64> {
    match bytes.len() {
        4 => Ok(u64::from(u32::from_be_bytes(
            bytes.try_into().map_err(|_| Error::InvalidArgs)?,
        ))),
        8 => Ok(u64::from_be_bytes(
            bytes.try_into().map_err(|_| Error::InvalidArgs)?,
        )),
        _ => Err(Error::InvalidArgs),
    }
}

fn checked_pointer_width<T>(value: u64) -> Result<T>
where
    T: TryFrom<u64>,
{
    T::try_from(value).map_err(|_| Error::Overflow)
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
                description.write_handoff_tree(reservation, &[]).unwrap(),
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
                description.write_handoff_tree(reservation, &[]),
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
        let description = unsafe { PlatformDescription::from_raw(address, destination).unwrap() };
        let reservation =
            PhysAddrRange::from_start_len(PhysAddr::new(0x8000_0000), 0x1000).unwrap();
        assert_eq!(
            description.write_handoff_tree(Some(reservation), &[]),
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
        description.write_handoff_tree(None, &[]).unwrap();
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

    #[test]
    fn register_cells_decode_the_full_address_without_truncation() {
        assert_eq!(decode_register_cells(&0x1000u32.to_be_bytes()), Ok(0x1000));
        assert_eq!(
            decode_register_cells(&0x1_0000_1000u64.to_be_bytes()),
            Ok(0x1_0000_1000)
        );
        assert_eq!(decode_register_cells(&[]), Err(Error::InvalidArgs));
        assert_eq!(decode_register_cells(&[0; 12]), Err(Error::InvalidArgs));
    }

    #[test]
    fn rv32_pointer_width_rejects_values_above_32_bits() {
        // Model RV32 on 64-bit hosts so this regression runs in ordinary tests.
        let address = decode_register_cells(&0x1_0000_1000u64.to_be_bytes()).unwrap();
        assert_eq!(checked_pointer_width::<u32>(address), Err(Error::Overflow));
        assert_eq!(checked_pointer_width::<u64>(address), Ok(0x1_0000_1000));
    }

    #[cfg(target_pointer_width = "32")]
    #[test]
    fn rv32_register_rejects_addresses_above_pointer_width() {
        assert_eq!(
            decode_register_value(&0x1_0000_1000u64.to_be_bytes()),
            Err(Error::Overflow)
        );
    }

    #[test]
    fn read_memory_ranges_from_dtb_skips_zero_length_before_rv32_conversion() {
        let mut source = memory_fdt(
            &[(0x1_0000_1000, 0), (0x8000_0000, 0x1000)],
            &[(0x1_1000_0000, 0), (0x9000_0000, 0x1000)],
        );
        let (_, description) = description_from_dtb(&mut source);

        let (ram, reserved) = description
            .read_memory_ranges_using(rv32_pointer_width)
            .unwrap();
        assert_eq!(ram, [test_range(0x8000_0000, 0x1000)]);
        assert_eq!(reserved, [test_range(0x9000_0000, 0x1000)]);

        // The production entry point follows the same DTB parsing path.
        assert_eq!(description.read_memory_ranges().unwrap(), (ram, reserved));
    }

    #[test]
    fn read_memory_ranges_from_dtb_rejects_nonempty_rv32_overflow() {
        let mut ram_source = memory_fdt(&[(0x1_0000_1000, 0x1000)], &[]);
        let (_, ram_description) = description_from_dtb(&mut ram_source);
        assert_eq!(
            ram_description
                .read_memory_ranges_using(rv32_pointer_width)
                .unwrap_err(),
            Error::Overflow
        );

        let mut reserved_source = memory_fdt(&[(0x8000_0000, 0x1000)], &[(0x1_1000_0000, 0x1000)]);
        let (_, reserved_description) = description_from_dtb(&mut reserved_source);
        assert_eq!(
            reserved_description
                .read_memory_ranges_using(rv32_pointer_width)
                .unwrap_err(),
            Error::Overflow
        );
    }

    fn rv32_pointer_width(value: u64) -> Result<usize> {
        u32::try_from(value)
            .map(|value| value as usize)
            .map_err(|_| Error::Overflow)
    }

    fn test_range(address: usize, size: usize) -> PhysAddrRange {
        PhysAddrRange::from_start_len(PhysAddr::new(address), size).unwrap()
    }

    fn description_from_dtb(source: &mut [u8]) -> (PhysAddrRange, PlatformDescription) {
        let address = PhysAddr::new(source.as_mut_ptr() as usize);
        let destination = PhysAddrRange::from_start_len(address, source.len()).unwrap();
        // SAFETY: the fixture remains live while the description is used, and
        // this test only reads its memory ranges.
        let description = unsafe { PlatformDescription::from_raw(address, destination).unwrap() };
        (destination, description)
    }

    fn write_test_u32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
    }

    fn memory_fdt(ram: &[(u64, u64)], reserved: &[(u64, u64)]) -> Vec<u8> {
        let mut strings = Vec::new();
        let address_cells = append_test_string(&mut strings, "#address-cells");
        let size_cells = append_test_string(&mut strings, "#size-cells");
        let ranges = append_test_string(&mut strings, "ranges");
        let reg = append_test_string(&mut strings, "reg");

        let mut structure = Vec::new();
        push_test_begin_node(&mut structure, "");
        push_test_property(&mut structure, address_cells, &2u32.to_be_bytes());
        push_test_property(&mut structure, size_cells, &2u32.to_be_bytes());
        for &(address, size) in ram {
            push_test_begin_node(&mut structure, "memory");
            push_test_property(&mut structure, reg, &test_reg(address, size));
            push_test_u32(&mut structure, 2);
        }
        if !reserved.is_empty() {
            push_test_begin_node(&mut structure, "reserved-memory");
            push_test_property(&mut structure, address_cells, &2u32.to_be_bytes());
            push_test_property(&mut structure, size_cells, &2u32.to_be_bytes());
            push_test_property(&mut structure, ranges, &[]);
            for &(address, size) in reserved {
                push_test_begin_node(&mut structure, "reserved");
                push_test_property(&mut structure, reg, &test_reg(address, size));
                push_test_u32(&mut structure, 2);
            }
            push_test_u32(&mut structure, 2);
        }
        push_test_u32(&mut structure, 2);
        push_test_u32(&mut structure, 9);

        let mut output = alloc::vec![0; 40];
        let reservations_offset = output.len();
        output.extend_from_slice(&[0; 16]);
        let structure_offset = output.len();
        output.extend_from_slice(&structure);
        let strings_offset = output.len();
        output.extend_from_slice(&strings);
        pad_test_bytes(&mut output);
        let total_size = output.len();

        write_test_u32(&mut output, 0, 0xd00d_feed);
        write_test_u32(&mut output, 4, total_size as u32);
        write_test_u32(&mut output, 8, structure_offset as u32);
        write_test_u32(&mut output, 12, strings_offset as u32);
        write_test_u32(&mut output, 16, reservations_offset as u32);
        write_test_u32(&mut output, 20, 17);
        write_test_u32(&mut output, 24, 16);
        write_test_u32(&mut output, 32, strings.len() as u32);
        write_test_u32(&mut output, 36, structure.len() as u32);
        output
    }

    fn append_test_string(strings: &mut Vec<u8>, value: &str) -> u32 {
        let offset = strings.len() as u32;
        strings.extend_from_slice(value.as_bytes());
        strings.push(0);
        offset
    }

    fn push_test_begin_node(structure: &mut Vec<u8>, name: &str) {
        push_test_u32(structure, 1);
        structure.extend_from_slice(name.as_bytes());
        structure.push(0);
        pad_test_bytes(structure);
    }

    fn push_test_property(structure: &mut Vec<u8>, name_offset: u32, value: &[u8]) {
        push_test_u32(structure, 3);
        push_test_u32(structure, value.len() as u32);
        push_test_u32(structure, name_offset);
        structure.extend_from_slice(value);
        pad_test_bytes(structure);
    }

    fn push_test_u32(bytes: &mut Vec<u8>, value: u32) {
        bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn test_reg(address: u64, size: u64) -> Vec<u8> {
        let mut value = Vec::new();
        value.extend_from_slice(&address.to_be_bytes());
        value.extend_from_slice(&size.to_be_bytes());
        value
    }

    fn pad_test_bytes(bytes: &mut Vec<u8>) {
        while !bytes.len().is_multiple_of(4) {
            bytes.push(0);
        }
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

    #[test]
    fn traverses_max_depth_fdt_iteratively_and_prunes_disabled_subtrees() {
        fn push_u32(buf: &mut Vec<u8>, value: u32) {
            buf.extend_from_slice(&value.to_be_bytes());
        }
        fn push_cstr_aligned(buf: &mut Vec<u8>, text: &[u8]) {
            buf.extend_from_slice(text);
            buf.push(0);
            while !buf.len().is_multiple_of(4) {
                buf.push(0);
            }
        }

        let strings = b"status\0";
        let mut structure = Vec::new();
        // Root node (level 0 -> depth 1).
        push_u32(&mut structure, patch::format::BEGIN_NODE);
        push_cstr_aligned(&mut structure, b"");

        // Disabled subtree whose child lacks `status`; pruning must skip both.
        push_u32(&mut structure, patch::format::BEGIN_NODE);
        push_cstr_aligned(&mut structure, b"disabled-sub");
        push_u32(&mut structure, patch::format::PROPERTY);
        push_u32(&mut structure, 9);
        push_u32(&mut structure, 0);
        push_cstr_aligned(&mut structure, b"disabled");
        push_u32(&mut structure, patch::format::BEGIN_NODE);
        push_cstr_aligned(&mut structure, b"hidden-child");
        push_u32(&mut structure, patch::format::END_NODE);
        push_u32(&mut structure, patch::format::END_NODE);

        // Chain reaching MAX_FDT_DEPTH = 63 total levels (`/` + `n1`..`n62`).
        for level in 1..=62 {
            push_u32(&mut structure, patch::format::BEGIN_NODE);
            let name = alloc::format!("n{level}");
            push_cstr_aligned(&mut structure, name.as_bytes());
        }
        for _ in 0..=62 {
            push_u32(&mut structure, patch::format::END_NODE);
        }
        push_u32(&mut structure, patch::format::END);

        let reservations_offset = patch::format::HEADER_SIZE;
        let structure_offset = reservations_offset + patch::format::RESERVATION_ENTRY_SIZE;
        let strings_offset = structure_offset + structure.len();
        let total_size = strings_offset + strings.len();

        let mut backing = alloc::vec![0u64; total_size.div_ceil(size_of::<u64>())];
        // SAFETY: `backing` is zero-initialized and covers `total_size` bytes.
        let dtb = unsafe {
            core::slice::from_raw_parts_mut(backing.as_mut_ptr().cast::<u8>(), total_size)
        };
        write_test_u32(dtb, 0, patch::format::MAGIC);
        write_test_u32(dtb, 4, total_size as u32);
        write_test_u32(dtb, 8, structure_offset as u32);
        write_test_u32(dtb, 12, strings_offset as u32);
        write_test_u32(dtb, 16, reservations_offset as u32);
        write_test_u32(dtb, 20, patch::format::SUPPORTED_VERSION);
        write_test_u32(dtb, 24, 16);
        write_test_u32(dtb, 32, strings.len() as u32);
        write_test_u32(dtb, 36, structure.len() as u32);
        dtb[structure_offset..strings_offset].copy_from_slice(&structure);
        dtb[strings_offset..total_size].copy_from_slice(strings);

        let address = PhysAddr::new(dtb.as_ptr() as usize);
        let destination = PhysAddrRange::from_start_len(address, total_size).unwrap();
        // SAFETY: `backing` remains live and immutable across `description` lifetime.
        let description = unsafe { PlatformDescription::from_raw(address, destination).unwrap() };

        let mut visited = 0usize;
        let mut max_depth = 0usize;
        let mut leaf_parent_is_n61 = false;
        description
            .inspect(|platform| {
                platform.try_for_each_enabled_node(|node, parent, path| {
                    visited += 1;
                    max_depth = max_depth.max(path.len());
                    assert_ne!(node.name, "disabled-sub");
                    assert_ne!(node.name, "hidden-child");
                    if node.name == "n62" {
                        assert_eq!(path.len(), 63);
                        assert_eq!(path[0], "/");
                        assert_eq!(path[61], "n61");
                        assert_eq!(path[62], "n62");
                        leaf_parent_is_n61 = parent.is_some_and(|p| p.name == "n61");
                    }
                    Ok(())
                })
            })
            .unwrap();

        assert_eq!(visited, 63);
        assert_eq!(max_depth, 63);
        assert!(leaf_parent_is_n61);
    }
}
