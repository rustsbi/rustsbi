//! Policy for the platform description passed to the next stage.
//!
//! Discovery records selected resources; this module decides which memory and
//! nodes must be hidden after firmware has selected and bound its devices.

use runtime::PlatformDescription;
use runtime::memory::{MemoryRegistry, PhysAddr};

/// Prepares the next-stage FDT from the bound platform policy.
pub(super) fn prepare_device_tree(
    memory: &MemoryRegistry,
    hidden_node_paths: &[&str],
    device_tree: PlatformDescription,
) -> runtime::Result<PhysAddr> {
    let firmware_reservation =
        (!memory.firmware_is_reserved()).then(|| memory.firmware_image_range());
    device_tree.prepare_next_stage(
        memory.firmware_ram_bank()?,
        firmware_reservation,
        hidden_node_paths,
    )
}
