//! Boot-local device-tree interpretation and selected-device binding.

use core::ops::Range;

use dtoolkit::{Node, Property};

mod aia;
mod clint;
mod console;
mod dt;
mod hart;
mod interrupts;
mod power;

pub(crate) use console::install as install_console;
pub(crate) use hart::discover as discover_harts;
pub(crate) use interrupts::install as install_interrupts;
pub(crate) use power::install as install_power;

/// Cold-boot rejection while firmware turns its owned DTB into capabilities.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Error {
    DeviceTree,
    Memory,
    HartCount,
    HartId,
    DuplicateHart,
    DeviceRange,
    AmbiguousDevice,
    UnsupportedDevice,
    Installation,
}

impl From<dt::DtbError> for Error {
    fn from(_: dt::DtbError) -> Self {
        Self::DeviceTree
    }
}

/// Parses the owned boot tree once for the complete firmware transaction.
pub(crate) fn parse(boot: &machine::BootDtb) -> Result<dt::BootTree, Error> {
    dt::BootTree::parse(boot).map_err(Into::into)
}

/// Finds the first enabled physical-memory range used for supervisor policy.
pub(crate) fn memory(tree: &dt::BootTree) -> Result<Range<usize>, Error> {
    let root = &tree.tree().root;
    let address_cells = dt::cell_count(root, "#address-cells", 2)?;
    let size_cells = dt::cell_count(root, "#size-cells", 1)?;

    for node in root.children() {
        let is_memory = node.name_without_address() == "memory"
            || node
                .property("device_type")
                .and_then(|property| property.as_str().ok())
                == Some("memory");
        if !is_memory || !dt::enabled(node) {
            continue;
        }
        let Some(reg) = node.property("reg") else {
            continue;
        };
        let Some((start, size)) = dt::first_reg(reg.value(), address_cells, size_cells) else {
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
pub(crate) fn finish_device_tree(
    tree: dt::BootTree,
    boot: &mut machine::BootDtb,
) -> Result<(), Error> {
    tree.finish(boot).map_err(Into::into)
}
