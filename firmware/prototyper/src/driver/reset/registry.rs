//! Registration and selection of reset drivers.
//!
//! This lifecycle is deliberately scoped to one device class. Reset discovery
//! needs parent Devicetree nodes and reset-specific bind resources; those
//! details do not belong in a universal firmware-driver trait.

use alloc::{boxed::Box, vec, vec::Vec};

use runtime::memory::{DeviceRegisterRange, MemoryRegistry};
use runtime::{Error, Result};
use serde_device_tree::buildin::Node;

use super::{ResetBackend, pmic_spacemit_p1, sifive_test, sunxi_watchdog, syscon};

/// Resources available while a matched reset driver binds its device.
pub(super) struct BindResources<'a> {
    memory: &'a mut MemoryRegistry,
    timebase_frequency_hz: Option<u32>,
}

impl<'a> BindResources<'a> {
    pub(super) fn new(memory: &'a mut MemoryRegistry, timebase_frequency_hz: Option<u32>) -> Self {
        Self {
            memory,
            timebase_frequency_hz,
        }
    }

    pub(super) fn memory(&mut self) -> &mut MemoryRegistry {
        self.memory
    }

    pub(super) const fn timebase_frequency_hz(&self) -> Option<u32> {
        self.timebase_frequency_hz
    }
}

/// One reset driver registered with the reset subsystem.
pub(super) trait ResetDriver: Send + Sync {
    /// Probes one enabled node and updates the driver's private description.
    fn probe(
        &mut self,
        platform: &runtime::PlatformView<'_>,
        node: &Node<'_>,
        parent: Option<&Node<'_>>,
    ) -> Result<()>;

    /// Returns whether this driver found a complete device description.
    fn has_device(&self) -> bool;

    /// Claims the matched resources and constructs the reset backend.
    fn bind(&self, resources: &mut BindResources<'_>) -> Result<Box<dyn ResetBackend>>;

    /// Logs the matched device description.
    fn log_summary(&self);
}

/// Reset drivers ordered from highest to lowest selection priority.
pub(super) struct Registry {
    drivers: Vec<Box<dyn ResetDriver>>,
}

impl Registry {
    pub(super) const fn empty() -> Self {
        Self {
            drivers: Vec::new(),
        }
    }

    pub(super) fn new() -> Self {
        let mut drivers = vec![
            Box::new(sifive_test::SifiveTestDriver::default()) as Box<dyn ResetDriver>,
            Box::new(pmic_spacemit_p1::P1PmicDriver::default()),
        ];
        drivers.extend(sunxi_watchdog::built_in_drivers());
        drivers.push(Box::new(syscon::SysconDriver::default()));
        Self { drivers }
    }

    pub(super) fn probe(
        &mut self,
        platform: &runtime::PlatformView<'_>,
        node: &Node<'_>,
        parent: Option<&Node<'_>>,
    ) -> Result<()> {
        for driver in &mut self.drivers {
            driver.probe(platform, node, parent)?;
        }
        Ok(())
    }

    /// Binds only the first matching driver.
    pub(super) fn bind_first(
        &self,
        resources: &mut BindResources<'_>,
    ) -> Result<Option<Box<dyn ResetBackend>>> {
        self.drivers
            .iter()
            .find(|driver| driver.has_device())
            .map(|driver| driver.bind(resources))
            .transpose()
    }

    /// Logs the first matching driver and reports whether one was found.
    pub(super) fn log_selected(&self) -> bool {
        let Some(driver) = self.drivers.iter().find(|driver| driver.has_device()) else {
            return false;
        };
        driver.log_summary();
        true
    }
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
    node: &Node<'_>,
) -> Result<DeviceRegisterRange> {
    platform
        .device_registers(node)?
        .and_then(|ranges| ranges.first().copied())
        .ok_or(Error::InvalidArgs)
}
