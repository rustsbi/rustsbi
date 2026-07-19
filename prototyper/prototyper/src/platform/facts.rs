//! Immutable platform facts derived once during cold boot.

#![expect(
    dead_code,
    reason = "complete validated facts are retained for DT policy and construction cross-checks"
)]

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::ops::Range;

use dtoolkit::{Node, Property};

use super::aia_facts::{self, Aplic, Imsic};
use super::clint_facts::{self, Clint};
use super::console_facts::{self, Console};
use super::dt::{DtbError, PlatformDtb, cell_count, enabled, first_reg};
use super::hart::{self, HartInfo};
use super::power::{self, Power};

/// Facts discovered from the owned boot tree.
///
/// This boot-local value contains no live device, SBI service, readiness flag,
/// MMIO authority, or machine capability. `main` consumes its fields into
/// responsibility-specific machine constructors.
pub struct Platform {
    /// Human-readable root model, if supplied.
    pub model: String,
    /// The first enabled physical memory range used for initial policy.
    pub memory: Range<usize>,
    /// Enabled architectural harts in device-tree order.
    pub harts: Vec<HartInfo>,
    /// Selected inert CLINT facts, if the tree contains a supported binding.
    pub clint: Option<Clint>,
    /// Selected inert firmware-console facts, if configured.
    pub console: Option<Console>,
    /// Selected inert whole-machine power-control facts, if available.
    pub power: Option<Power>,
    /// Retained machine-level IMSIC facts, if a complete AIA path exists.
    pub imsic: Option<Imsic>,
    /// Retained machine-level APLIC facts, if a complete AIA path exists.
    pub aplic: Option<Aplic>,
}

/// Errors from safe platform discovery and supervisor-DT preparation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiscoverError {
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
}

impl From<DtbError> for DiscoverError {
    fn from(_: DtbError) -> Self {
        Self::DeviceTree
    }
}

/// Discovers immutable facts from the machine-owned boot tree.
pub fn discover(boot: &machine::BootDtb) -> Result<Platform, DiscoverError> {
    let tree = PlatformDtb::parse(boot)?;
    let model = root_model(tree.tree().root.property("model"));
    let memory = discover_memory(&tree)?;
    let harts = hart::discover(&tree)?;
    let (imsic, aplic) = aia_facts::discover(&tree, &harts)?;
    let clint = clint_facts::discover(&tree.tree().root)?;
    let console = console_facts::discover(&tree.tree().root)?;
    let power = power::discover(&tree.tree().root)?;

    Ok(Platform {
        model,
        memory,
        harts,
        clint,
        console,
        power,
        imsic,
        aplic,
    })
}

impl Platform {
    /// Applies supervisor visibility policy after all selected machine devices
    /// have been constructed from the original owned tree.
    pub fn prepare_supervisor_dtb(&self, boot: &mut machine::BootDtb) -> Result<(), DiscoverError> {
        let mut tree = PlatformDtb::parse(boot)?;
        // These exact edits and machine construction consume the same retained
        // node identities. A construction failure never calls this operation;
        // a successful one cannot leave machine-owned MMIO enabled for S-mode.
        if let Some(device) = &self.clint {
            tree.remove_node(&device.path)?;
        }
        if let Some(device) = &self.aplic {
            tree.remove_node(&device.path)?;
        }
        if let Some(device) = &self.imsic {
            tree.remove_node(&device.path)?;
        }
        if let Some(device) = &self.console {
            tree.disable_node(&device.path)?;
        }
        if let Some(device) = &self.power {
            for consumer in &device.consumers {
                tree.disable_node(consumer)?;
            }
            tree.disable_node(&device.path)?;
        }
        tree.finish(boot)?;
        Ok(())
    }
}

fn root_model(property: Option<&dtoolkit::model::DeviceTreeProperty>) -> String {
    property
        .and_then(|property| property.as_str().ok())
        .map(ToString::to_string)
        .unwrap_or_default()
}

fn discover_memory(tree: &PlatformDtb) -> Result<Range<usize>, DiscoverError> {
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
        let start = usize::try_from(start).map_err(|_| DiscoverError::Memory)?;
        let size = usize::try_from(size).map_err(|_| DiscoverError::Memory)?;
        let end = start.checked_add(size).ok_or(DiscoverError::Memory)?;
        if start < end {
            return Ok(start..end);
        }
    }
    Err(DiscoverError::Memory)
}
