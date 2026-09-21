#![forbid(unsafe_code)]

use alloc::{string::String, vec::Vec};
use runtime::{Compatible, FdtNode};

/// One enabled node supplied by the shared discovery traversal.
///
/// The compatible property is retained as a view before the node is
/// dispatched to reset, vendor, and interrupt discovery.
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

/// The result of selecting one resource from an enabled source-tree node.
///
/// Unlike [`EnabledNode`], this is an owned discovery result: the selected
/// resource and the absolute source path can outlive the temporary FDT view.
pub(crate) struct NodeSelection<T> {
    resource: T,
    source_path: String,
}

impl<T> NodeSelection<T> {
    pub(crate) fn resource(&self) -> &T {
        &self.resource
    }

    pub(crate) fn source_path(&self) -> &str {
        &self.source_path
    }
}

pub(crate) fn select_from_node_once<T>(
    slot: &mut Option<NodeSelection<T>>,
    resource: T,
    source_path: &[&str],
) -> runtime::Result<()> {
    if slot.is_some() {
        return Err(runtime::Error::InvalidArgs);
    }
    slot.replace(NodeSelection {
        resource,
        source_path: absolute_path(source_path),
    });
    Ok(())
}

/// Converts a traversal path to an owned absolute device-tree path.
pub(crate) fn absolute_path(segments: &[&str]) -> String {
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

/// Returns whether the node is a CPU node under `/cpus`.
pub(crate) fn is_cpu_node(node: FdtNode<'_, '_>) -> bool {
    node.name.split('@').next() == Some("cpu")
}

/// Visits enabled nodes depth first without allocating a path.
///
/// This node-only form is kept for PMU discovery, whose callback does not
/// need the path or parent context carried by [`try_for_each_enabled_node`].
pub(crate) fn visit_enabled_nodes<'b, 'a, F>(root: FdtNode<'b, 'a>, visitor: &mut F)
where
    F: FnMut(FdtNode<'b, 'a>),
{
    if !runtime::node_is_enabled(root) {
        return;
    }
    visitor(root);
    for child in root.children() {
        visit_enabled_nodes(child, visitor);
    }
}

/// Tries to visit enabled nodes depth first, retaining each node's parent.
pub(crate) fn try_for_each_enabled_node<'b, 'a, F>(
    root: FdtNode<'b, 'a>,
    visitor: &mut F,
) -> runtime::Result<()>
where
    F: FnMut(EnabledNode<'b, 'a>, &[&str]) -> runtime::Result<()>,
{
    fn visit<'b, 'a, F>(
        node: FdtNode<'b, 'a>,
        parent: Option<FdtNode<'b, 'a>>,
        path: &mut Vec<&'a str>,
        visitor: &mut F,
    ) -> runtime::Result<()>
    where
        F: FnMut(EnabledNode<'b, 'a>, &[&str]) -> runtime::Result<()>,
    {
        if !runtime::node_is_enabled(node) {
            return Ok(());
        }

        path.push(node.name);
        visitor(
            EnabledNode {
                node,
                parent,
                compatible: node.compatible(),
            },
            path.as_slice(),
        )?;
        for child in node.children() {
            visit(child, Some(node), path, visitor)?;
        }
        path.pop();
        Ok(())
    }

    visit(root, None, &mut Vec::new(), visitor)
}
