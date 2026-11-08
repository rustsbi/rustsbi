//! Policy for the platform description passed to the next stage.
//!
//! Selected devices determine which memory and nodes must be hidden from the
//! next stage.

use runtime::{
    PlatformDescription, Result,
    memory::{MemoryRegistry, PhysAddr},
};

/// Prepares the next-stage FDT from the bound platform policy.
pub(super) fn prepare_device_tree(
    memory: &MemoryRegistry,
    hidden_node_paths: &[&str],
    device_tree: PlatformDescription,
) -> Result<PhysAddr> {
    let firmware_reservation =
        (!memory.firmware_is_reserved()).then(|| memory.firmware_image_range());
    device_tree.prepare_next_stage(firmware_reservation, hidden_node_paths)
}
