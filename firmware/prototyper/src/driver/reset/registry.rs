//! Reset-driver discovery and binding interface.
//!
//! Reset discovery needs parent Devicetree nodes and reset-specific resources,
//! so this interface is scoped to the reset device class.

use alloc::boxed::Box;

use runtime::memory::{DeviceRegisterRange, MemoryRegistry};
use runtime::{Error, FdtNode, Result};

use crate::devicetree::EnabledNode;

use super::ResetDevice;

/// One built-in reset driver in the reset subsystem.
pub(super) trait ResetDriver: Send + Sync {
    /// Probes one enabled node and updates the driver's private description.
    fn probe(
        &mut self,
        platform: &runtime::PlatformView<'_>,
        node: EnabledNode<'_, '_>,
    ) -> Result<()>;

    /// Returns whether this driver found a complete device description.
    fn has_device(&self) -> bool;

    /// Claims the matched resources and constructs the reset device.
    fn bind(
        &self,
        memory: &mut MemoryRegistry,
        timebase_frequency_hz: Option<u32>,
    ) -> Result<Box<dyn ResetDevice>>;

    /// Logs the matched device description.
    fn log_summary(&self);
}

/// Retains exactly one discovered value for a single-device description.
pub(super) fn set_once<T>(slot: &mut Option<T>, value: T) -> Result<()> {
    if slot.replace(value).is_some() {
        return Err(Error::InvalidArgs);
    }
    Ok(())
}

/// Reads the primary register range for an MMIO device node.
pub(super) fn primary_registers(
    platform: &runtime::PlatformView<'_>,
    node: FdtNode<'_, '_>,
) -> Result<DeviceRegisterRange> {
    platform.device_register(node)?.ok_or(Error::InvalidArgs)
}
