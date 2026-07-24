//! Safe owned device-tree adapter used only during cold-boot discovery.

use alloc::borrow::ToOwned;
use alloc::collections::BTreeSet;
use alloc::string::String;
use alloc::vec::Vec;
use core::ops::Range;

use dtoolkit::fdt::Fdt;
use dtoolkit::model::{DeviceTree, DeviceTreeNode, DeviceTreeProperty};
use dtoolkit::{Node, Property};

use super::config::{
    BOOT_DTB_MAX_DEPTH, BOOT_DTB_MAX_NODES, BOOT_DTB_MAX_PROPERTIES, BOOT_DTB_MAX_SIZE,
};

/// Owned, bounded semantic tree shared only by boot-local platform installers.
pub(crate) struct BootTree {
    tree: DeviceTree,
    boot_cpuid_phys: [u8; 4],
    edits: usize,
}

impl BootTree {
    /// Parses the machine-owned boot blob without constructing a raw-pointer
    /// view or retaining a dependency-specific node outside this adapter.
    pub(super) fn parse(boot: &machine::BootDtb) -> Result<Self, DtbError> {
        if boot.as_bytes().len() > BOOT_DTB_MAX_SIZE {
            return Err(DtbError::SizeLimit);
        }
        let fdt = Fdt::new(boot.as_bytes()).map_err(|_| DtbError::Malformed)?;
        validate_flat_tree(&fdt)?;
        let tree = DeviceTree::from_fdt(&fdt);
        validate_owned_tree(&tree)?;
        let mut boot_cpuid_phys = [0; 4];
        boot_cpuid_phys.copy_from_slice(&boot.as_bytes()[28..32]);
        Ok(Self {
            tree,
            boot_cpuid_phys,
            edits: 0,
        })
    }

    pub(super) fn tree(&self) -> &DeviceTree {
        &self.tree
    }

    pub(super) fn remove_node(&mut self, path: &str) -> Result<(), DtbError> {
        self.reserve_edit()?;
        let (parent, name) = parent_mut(&mut self.tree.root, path)?;
        parent.remove_child(name).ok_or(DtbError::Malformed)?;
        Ok(())
    }

    pub(super) fn disable_node(&mut self, path: &str) -> Result<(), DtbError> {
        self.reserve_edit()?;
        let node = node_mut_at_path(&mut self.tree.root, path).ok_or(DtbError::Malformed)?;
        let status = DeviceTreeProperty::new("status", b"disabled\0".to_vec())
            .map_err(|_| DtbError::Malformed)?;
        node.add_property(status);
        Ok(())
    }

    fn reserve_edit(&mut self) -> Result<(), DtbError> {
        self.edits = self.edits.checked_add(1).ok_or(DtbError::EditLimit)?;
        if self.edits > super::config::BOOT_DTB_MAX_EDITS {
            return Err(DtbError::EditLimit);
        }
        Ok(())
    }

    /// Encodes, independently reparses, and atomically replaces the backing
    /// allocation of the same logical `BootDtb` owner.
    pub(super) fn finish(self, boot: &mut machine::BootDtb) -> Result<(), DtbError> {
        // This exact bound covers every length converted to `u32` by the
        // encoder, so its documented length-overflow panic is unreachable.
        let encoded_size = validate_owned_tree(&self.tree)?;
        let mut encoded = self.tree.to_dtb();
        if encoded.len() != encoded_size {
            return Err(DtbError::EncoderOutput);
        }
        encoded[28..32].copy_from_slice(&self.boot_cpuid_phys);
        Fdt::new(&encoded).map_err(|_| DtbError::EncoderOutput)?;
        boot.replace_encoded(encoded)
            .map_err(|_| DtbError::EncoderOutput)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DtbError {
    Malformed,
    SizeLimit,
    DepthLimit,
    NodeLimit,
    PropertyLimit,
    EditLimit,
    SizeOverflow,
    EncoderOutput,
}

fn parent_mut<'a>(
    root: &'a mut DeviceTreeNode,
    path: &'a str,
) -> Result<(&'a mut DeviceTreeNode, &'a str), DtbError> {
    let path = path
        .strip_prefix('/')
        .filter(|path| !path.is_empty())
        .ok_or(DtbError::Malformed)?;
    let (parent_path, name) = path.rsplit_once('/').unwrap_or(("", path));
    let parent = if parent_path.is_empty() {
        root
    } else {
        node_mut_at_path(root, &alloc::format!("/{parent_path}")).ok_or(DtbError::Malformed)?
    };
    Ok((parent, name))
}

fn node_mut_at_path<'a>(
    root: &'a mut DeviceTreeNode,
    path: &str,
) -> Option<&'a mut DeviceTreeNode> {
    let mut current = root;
    for segment in path.strip_prefix('/')?.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return None;
        }
        current = current.child_mut(segment)?;
    }
    Some(current)
}

pub(super) fn enabled(node: &DeviceTreeNode) -> bool {
    node.property("status")
        .and_then(|property| property.as_str().ok())
        .is_none_or(|status| matches!(status, "ok" | "okay"))
}

pub(super) fn cell_count(
    node: &DeviceTreeNode,
    name: &str,
    default: usize,
) -> Result<usize, DtbError> {
    match node.property(name) {
        Some(property) => usize::try_from(property.as_u32().map_err(|_| DtbError::Malformed)?)
            .map_err(|_| DtbError::Malformed),
        None => Ok(default),
    }
}

pub(super) fn first_reg(
    value: &[u8],
    address_cells: usize,
    size_cells: usize,
) -> Option<(u64, u64)> {
    let address_bytes = address_cells.checked_mul(4)?;
    let size_bytes = size_cells.checked_mul(4)?;
    let entry_bytes = address_bytes.checked_add(size_bytes)?;
    if entry_bytes == 0 || value.len() < entry_bytes || !value.len().is_multiple_of(entry_bytes) {
        return None;
    }
    Some((
        read_cells(&value[..address_bytes], address_cells)?,
        read_cells(&value[address_bytes..entry_bytes], size_cells)?,
    ))
}

pub(super) fn read_cells(value: &[u8], count: usize) -> Option<u64> {
    if count == 0 {
        return Some(0);
    }
    if count > 2 || value.len() != count.checked_mul(4)? {
        return None;
    }
    let mut result = 0u64;
    for cell in value.chunks_exact(4) {
        result = result.checked_shl(32)?
            | u32::from_be_bytes([cell[0], cell[1], cell[2], cell[3]]) as u64;
    }
    Some(result)
}

pub(super) fn nodes_with_paths(root: &DeviceTreeNode) -> Vec<(&DeviceTreeNode, String)> {
    let mut result = Vec::new();
    let mut pending = Vec::new();
    for child in root.children() {
        pending.push((child, alloc::format!("/{}", child.name())));
    }
    while let Some((node, path)) = pending.pop() {
        for child in node.children() {
            pending.push((child, alloc::format!("{path}/{}", child.name())));
        }
        result.push((node, path));
    }
    result
}

pub(super) fn node_at_path<'a>(root: &'a DeviceTreeNode, path: &str) -> Option<&'a DeviceTreeNode> {
    let mut current = root;
    for segment in path.strip_prefix('/')?.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return None;
        }
        current = current.children().find(|node| node.name() == segment)?;
    }
    Some(current)
}

pub(super) fn reg_at_path(root: &DeviceTreeNode, path: &str) -> Result<Range<usize>, DtbError> {
    let mut parent = root;
    let mut segments = path
        .strip_prefix('/')
        .filter(|path| !path.is_empty())
        .ok_or(DtbError::Malformed)?
        .split('/')
        .peekable();
    loop {
        let segment = segments.next().ok_or(DtbError::Malformed)?;
        let node = parent
            .children()
            .find(|node| node.name() == segment)
            .ok_or(DtbError::Malformed)?;
        if segments.peek().is_none() {
            let address_cells = cell_count(parent, "#address-cells", 2)?;
            let size_cells = cell_count(parent, "#size-cells", 1)?;
            let reg = node.property("reg").ok_or(DtbError::Malformed)?;
            let (start, size) =
                first_reg(reg.value(), address_cells, size_cells).ok_or(DtbError::Malformed)?;
            let start = usize::try_from(start).map_err(|_| DtbError::Malformed)?;
            let size = usize::try_from(size).map_err(|_| DtbError::Malformed)?;
            let end = start.checked_add(size).ok_or(DtbError::Malformed)?;
            return (start < end)
                .then_some(start..end)
                .ok_or(DtbError::Malformed);
        }
        if node
            .property("ranges")
            .is_some_and(|ranges| !ranges.value().is_empty())
        {
            return Err(DtbError::Malformed);
        }
        parent = node;
    }
}

fn validate_flat_tree(fdt: &Fdt<'_>) -> Result<(), DtbError> {
    let root = fdt.root();
    let mut pending = Vec::new();
    pending.push((root, 1usize));
    let mut nodes = 0usize;
    let mut properties = 0usize;

    while let Some((node, depth)) = pending.pop() {
        update_counts(
            depth,
            &mut nodes,
            &mut properties,
            node.properties().count(),
        )?;
        for child in node.children() {
            pending.push((child, depth.checked_add(1).ok_or(DtbError::DepthLimit)?));
        }
    }
    Ok(())
}

fn validate_owned_tree(tree: &DeviceTree) -> Result<usize, DtbError> {
    let mut pending: Vec<(&DeviceTreeNode, usize)> = Vec::new();
    pending.push((&tree.root, 1));
    let mut nodes = 0usize;
    let mut properties = 0usize;
    let mut encoded_size = tree
        .memory_reservations
        .len()
        .checked_add(1)
        .and_then(|count| count.checked_mul(16))
        .and_then(|reservations| reservations.checked_add(40 + 4))
        .ok_or(DtbError::SizeOverflow)?;
    let mut property_names = BTreeSet::new();

    while let Some((node, depth)) = pending.pop() {
        update_counts(
            depth,
            &mut nodes,
            &mut properties,
            node.properties().count(),
        )?;

        // FDT_BEGIN_NODE, padded name including NUL, and FDT_END_NODE.
        checked_add(&mut encoded_size, 4)?;
        checked_add(&mut encoded_size, aligned_len(node.name().len(), true)?)?;
        checked_add(&mut encoded_size, 4)?;
        for property in node.properties() {
            // FDT_PROP, value length, name offset, and padded value.
            checked_add(&mut encoded_size, 12)?;
            checked_add(
                &mut encoded_size,
                aligned_len(property.value().len(), false)?,
            )?;
            property_names.insert(property.name().to_owned());
        }
        for child in node.children() {
            pending.push((child, depth.checked_add(1).ok_or(DtbError::DepthLimit)?));
        }
    }

    for name in property_names {
        checked_add(
            &mut encoded_size,
            name.len().checked_add(1).ok_or(DtbError::SizeOverflow)?,
        )?;
    }
    if encoded_size > BOOT_DTB_MAX_SIZE || u32::try_from(encoded_size).is_err() {
        return Err(DtbError::SizeLimit);
    }
    Ok(encoded_size)
}

fn update_counts(
    depth: usize,
    nodes: &mut usize,
    properties: &mut usize,
    node_properties: usize,
) -> Result<(), DtbError> {
    if depth > BOOT_DTB_MAX_DEPTH {
        return Err(DtbError::DepthLimit);
    }
    *nodes = nodes.checked_add(1).ok_or(DtbError::NodeLimit)?;
    if *nodes > BOOT_DTB_MAX_NODES {
        return Err(DtbError::NodeLimit);
    }

    *properties = properties
        .checked_add(node_properties)
        .ok_or(DtbError::PropertyLimit)?;
    if *properties > BOOT_DTB_MAX_PROPERTIES {
        return Err(DtbError::PropertyLimit);
    }
    Ok(())
}

fn aligned_len(length: usize, include_nul: bool) -> Result<usize, DtbError> {
    let length = if include_nul {
        length.checked_add(1).ok_or(DtbError::SizeOverflow)?
    } else {
        length
    };
    length
        .checked_add(3)
        .map(|length| length & !3)
        .ok_or(DtbError::SizeOverflow)
}

fn checked_add(total: &mut usize, value: usize) -> Result<(), DtbError> {
    *total = total.checked_add(value).ok_or(DtbError::SizeOverflow)?;
    Ok(())
}
