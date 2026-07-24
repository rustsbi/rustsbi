//! Safe discovery shared by the boot-local platform installers.

use core::ops::Range;

use dtoolkit::{Node, Property};

use super::dt::{BootTree, DtbError, cell_count, enabled, first_reg};

/// Boot-local failure while discovering or installing platform resources.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    /// The owned input or encoded output is not a valid bounded DTB.
    DeviceTree,
    /// No enabled, representable physical memory range was found.
    Memory,
    /// No enabled hart was found or the configured capacity was exceeded.
    HartCount,
    /// An enabled hart has a missing, malformed, or unrepresentable ID.
    HartId,
    /// Two enabled CPU nodes name the same physical hart.
    DuplicateHart,
    /// A recognized device has a malformed or unrepresentable register range.
    DeviceRange,
    /// More than one enabled device competes for a singleton machine role.
    AmbiguousDevice,
    /// A selected device has no retained concrete machine binding.
    UnsupportedDevice,
    /// A selected machine or ordinary driver rejected installation.
    Installation,
}

impl From<DtbError> for Error {
    fn from(_: DtbError) -> Self {
        Self::DeviceTree
    }
}

/// Parses the owned boot tree once for the complete upper boot transaction.
pub fn parse(boot: &machine::BootDtb) -> Result<BootTree, Error> {
    BootTree::parse(boot).map_err(Into::into)
}

/// Finds the first enabled physical-memory range used for supervisor policy.
pub fn memory(tree: &BootTree) -> Result<Range<usize>, Error> {
    let root = &tree.tree().root;
    let address_cells = cell_count(root, "#address-cells", 2)?;
    let size_cells = cell_count(root, "#size-cells", 1)?;

    for node in root.children() {
        let is_memory = node.name_without_address() == "memory"
            || node
                .property("device_type")
                .and_then(|property| property.as_str().ok())
                == Some("memory");
        if !is_memory || !enabled(node) {
            continue;
        }
        let Some(reg) = node.property("reg") else {
            continue;
        };
        let Some((start, size)) = first_reg(reg.value(), address_cells, size_cells) else {
            continue;
        };
        let start = usize::try_from(start).map_err(|_| Error::Memory)?;
        let size = usize::try_from(size).map_err(|_| Error::Memory)?;
        let end = start.checked_add(size).ok_or(Error::Memory)?;
        if start < end {
            return Ok(start..end);
        }
    }
    Err(Error::Memory)
}

/// Publishes all recorded visibility edits into the supervisor DTB.
pub fn finish(tree: BootTree, boot: &mut machine::BootDtb) -> Result<(), Error> {
    tree.finish(boot).map_err(Into::into)
}
