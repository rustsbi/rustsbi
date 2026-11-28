#![forbid(unsafe_code)]

use alloc::{string::String, vec::Vec};
use runtime::{Error, FdtNode, PlatformView, Result, hart::HartId, node_is_enabled};

/// A discovered resource and its absolute path in the source device tree.
///
/// The owned path lets the next-stage handoff omit the node after the
/// discovery view is released.
pub(crate) struct NodeSelection<T> {
    resource: T,
    source_path: String,
}

impl<T> NodeSelection<T> {
    pub(crate) fn new(resource: T, source_path: &[&str]) -> Self {
        Self {
            resource,
            source_path: absolute_path(source_path),
        }
    }

    pub(crate) fn resource(&self) -> &T {
        &self.resource
    }

    /// Returns the resource and the device-tree path that described it.
    pub(crate) fn into_parts(self) -> (T, String) {
        (self.resource, self.source_path)
    }
}

fn absolute_path(segments: &[&str]) -> String {
    let mut path = String::new();
    for segment in segments {
        if *segment == "/" || segment.is_empty() {
            continue;
        }
        path.push('/');
        path.push_str(segment);
    }
    if path.is_empty() {
        String::from("/")
    } else {
        path
    }
}

/// Reads one big-endian 32-bit property.
pub(crate) fn u32_property(node: FdtNode<'_, '_>, name: &str) -> Option<u32> {
    Some(u32::from_be_bytes(
        node.property(name)?.value.try_into().ok()?,
    ))
}

pub(crate) fn is_compatible(node: FdtNode<'_, '_>, compatible: &str) -> bool {
    node.compatible()
        .is_some_and(|names| names.all().any(|name| name == compatible))
}

/// Decodes a property composed of fixed-size rows of big-endian cells.
pub(crate) fn property_rows<const COLUMNS: usize>(
    bytes: &[u8],
) -> Option<impl Iterator<Item = [u32; COLUMNS]> + '_> {
    let (cells, trailing_bytes) = bytes.as_chunks::<4>();
    let (rows, trailing_cells) = cells.as_chunks::<COLUMNS>();
    (trailing_bytes.is_empty() && trailing_cells.is_empty())
        .then(|| rows.iter().map(|row| row.map(u32::from_be_bytes)))
}

/// One resolved pair from a node's `interrupts-extended` property.
///
/// `slot` retains the pair's original position for ACLINT register and IMSIC
/// file indexing. `hart` is `None` if the CPU has no enabled hart record.
pub(crate) struct CpuInterruptSource {
    pub(crate) hart: Option<HartId>,
    pub(crate) controller_enabled: bool,
    pub(crate) interrupt_id: u32,
    pub(crate) slot: u32,
}

/// Resolves each `interrupts-extended` phandle to its CPU and hart position.
pub(crate) fn resolve_interrupts_extended(
    platform: &PlatformView<'_>,
    node: FdtNode<'_, '_>,
) -> Result<Vec<CpuInterruptSource>> {
    let property = node
        .property("interrupts-extended")
        .ok_or(Error::InvalidArgs)?;
    if property.value.is_empty() {
        return Err(Error::InvalidArgs);
    }
    let entries = property_rows::<2>(property.value).ok_or(Error::InvalidArgs)?;
    let controllers = cpu_interrupt_controllers(platform)?;
    entries
        .enumerate()
        .map(|(slot, [phandle, interrupt])| {
            let controller = controllers
                .iter()
                .find(|controller| controller.phandle == phandle)
                .ok_or(Error::InvalidArgs)?;
            Ok(CpuInterruptSource {
                hart: HartId::from_raw(controller.hart_id).ok(),
                controller_enabled: controller.enabled,
                interrupt_id: interrupt,
                slot: u32::try_from(slot).map_err(|_| Error::InvalidArgs)?,
            })
        })
        .collect()
}

/// Checks the `cpu` base name of a node selected from `/cpus` by the caller.
pub(crate) fn is_cpu_node(node: FdtNode<'_, '_>) -> bool {
    node.name.split('@').next() == Some("cpu")
}

struct CpuInterruptController {
    phandle: u32,
    hart_id: usize,
    enabled: bool,
}

fn cpu_interrupt_controllers(platform: &PlatformView<'_>) -> Result<Vec<CpuInterruptController>> {
    let cpus = platform
        .find_enabled_node("/cpus")
        .ok_or(Error::InvalidArgs)?;
    let mut controllers = Vec::new();
    for cpu in cpus.children().filter(|node| is_cpu_node(*node)) {
        let hart_id = PlatformView::hart_id(cpu)?;
        controllers.extend(cpu.children().filter_map(|node| {
            if node.name.split('@').next() != Some("interrupt-controller")
                || !is_compatible(node, "riscv,cpu-intc")
            {
                return None;
            }
            let phandle =
                u32_property(node, "phandle").or_else(|| u32_property(node, "linux,phandle"))?;
            Some(CpuInterruptController {
                phandle,
                hart_id,
                enabled: node_is_enabled(node),
            })
        }));
    }
    Ok(controllers)
}
