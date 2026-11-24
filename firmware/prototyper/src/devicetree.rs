#![forbid(unsafe_code)]

use alloc::{string::String, vec::Vec};
use core::mem::size_of;
use runtime::{Compatible, Error, FdtNode, PlatformView, Result, hart::HartId, node_is_enabled};

/// A device-tree node whose status and ancestors allow discovery.
#[derive(Clone, Copy)]
pub(crate) struct EnabledNode<'view, 'tree: 'view> {
    node: FdtNode<'view, 'tree>,
    parent: Option<FdtNode<'view, 'tree>>,
    compatible: Option<Compatible<'tree>>,
}

impl<'view, 'tree> EnabledNode<'view, 'tree> {
    pub(crate) const fn node(self) -> FdtNode<'view, 'tree> {
        self.node
    }

    pub(crate) const fn parent(self) -> Option<FdtNode<'view, 'tree>> {
        self.parent
    }

    pub(crate) const fn compatible(self) -> Option<Compatible<'tree>> {
        self.compatible
    }
}

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
    let bytes = node.property(name)?.value;
    Some(u32::from_be_bytes(bytes.try_into().ok()?))
}

/// Decodes a property containing a list of big-endian 32-bit cells.
pub(crate) fn u32_cells(node: FdtNode<'_, '_>, name: &str) -> Option<Vec<u32>> {
    let bytes = node.property(name)?.value;
    let mut cells = Vec::new();
    let mut chunks = bytes.chunks_exact(size_of::<u32>());
    for chunk in &mut chunks {
        cells.push(u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }
    chunks.remainder().is_empty().then_some(cells)
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
    let cells = u32_cells(node, "interrupts-extended").ok_or(Error::InvalidArgs)?;
    if cells.is_empty() || !cells.len().is_multiple_of(2) {
        return Err(Error::InvalidArgs);
    }
    let controllers = cpu_interrupt_controllers(platform)?;
    cells
        .chunks_exact(2)
        .enumerate()
        .map(|(slot, interrupt)| {
            let controller = controllers
                .iter()
                .find(|controller| controller.phandle == interrupt[0])
                .ok_or(Error::InvalidArgs)?;
            Ok(CpuInterruptSource {
                hart: HartId::from_raw(controller.hart_id).ok(),
                controller_enabled: controller.enabled,
                interrupt_id: interrupt[1],
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
                || !node.compatible().is_some_and(|compatibles| {
                    compatibles
                        .all()
                        .any(|device_id| device_id == "riscv,cpu-intc")
                })
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

/// Visits enabled nodes depth first, skipping disabled subtrees.
///
/// The visitor receives the node, its parent, and its path from the root.
pub(crate) fn try_for_each_enabled_node<'view, 'tree: 'view>(
    platform: &'view PlatformView<'tree>,
    mut visitor: impl FnMut(EnabledNode<'view, 'tree>, &[&'tree str]) -> Result<()>,
) -> Result<()> {
    platform.try_for_each_enabled_node(|node, parent, path| {
        visitor(
            EnabledNode {
                node,
                parent,
                compatible: node.compatible(),
            },
            path,
        )
    })
}
