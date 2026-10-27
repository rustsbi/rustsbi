//! Generic driver table and deferred device binding.
//!
//! A class owns one [`Drivers`] table. Each entry is a [`Driver`] that parses
//! one device-tree node into a [`Description`]. The description carries the
//! parsed facts and a deferred `bind`; only `bind` receives the
//! [`MemoryRegistry`], so parsing cannot acquire MMIO.

use alloc::boxed::Box;
use runtime::{
    PlatformView, Result,
    memory::{DeviceRegisterRange, MemoryRegistry},
};

use crate::devicetree::{EnabledNode, try_for_each_enabled_node};

/// A device class: the type-level relationship a driver belongs to.
pub(crate) trait Class: 'static {
    /// The object a driver of this class produces.
    type Device;
}

/// A driver that can produce a device for class `C`.
pub(crate) trait Driver<C: Class>: Send + Sync + 'static {
    /// Device-tree `compatible` strings this driver claims.
    fn compatibles(&self) -> &'static [&'static str];

    /// Parses one enabled node into a description without acquiring MMIO.
    ///
    /// `Ok(None)` means the compatible matched but this driver declines the
    /// node, for example an 8250 with a 32-bit register layout.
    fn parse<'a, 'tree>(
        &self,
        platform: &'a PlatformView<'tree>,
        device: EnabledNode<'a, 'tree>,
        compatible: &str,
    ) -> Result<Option<Description<C>>>;
}

type Bind<C> = Box<dyn FnOnce(&mut MemoryRegistry) -> Result<<C as Class>::Device>>;

/// Name and register window of one bound class device.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DeviceSummary {
    pub(crate) name: &'static str,
    pub(crate) registers: DeviceRegisterRange,
}

/// A parsed device that has not acquired its MMIO range yet.
pub(crate) struct Description<C: Class> {
    name: &'static str,
    registers: DeviceRegisterRange,
    bind: Bind<C>,
}

impl<C: Class> Description<C> {
    pub(crate) fn new(
        name: &'static str,
        registers: DeviceRegisterRange,
        bind: impl FnOnce(&mut MemoryRegistry) -> Result<C::Device> + 'static,
    ) -> Self {
        Self {
            name,
            registers,
            bind: Box::new(bind),
        }
    }

    /// Driver name retained for boot reporting.
    pub(crate) fn name(&self) -> &'static str {
        self.name
    }

    /// Register window retained for boot reporting.
    pub(crate) fn registers(&self) -> DeviceRegisterRange {
        self.registers
    }

    /// Summary retained after binding.
    pub(crate) fn summary(&self) -> DeviceSummary {
        DeviceSummary {
            name: self.name,
            registers: self.registers,
        }
    }

    /// Acquires the MMIO range and constructs the live device.
    pub(crate) fn bind(self, memory: &mut MemoryRegistry) -> Result<C::Device> {
        (self.bind)(memory)
    }
}

/// An ordered list of drivers for one class.
pub(crate) struct Drivers<C: Class> {
    entries: &'static [&'static dyn Driver<C>],
}

impl<C: Class> Drivers<C> {
    pub(crate) const fn new(entries: &'static [&'static dyn Driver<C>]) -> Self {
        Self { entries }
    }

    /// Parses the first matching node, preferring `preferred` when present.
    ///
    /// Within one node, `compatible` strings are tried in device-tree order;
    /// for each compatible, drivers are tried in table order. This keeps a
    /// specific compatible ahead of a generic fallback while preserving the
    /// driver table's order for drivers claiming the same compatible.
    pub(crate) fn select<'a, 'tree>(
        &self,
        platform: &'a PlatformView<'tree>,
        preferred: Option<runtime::FdtNode<'a, 'tree>>,
    ) -> Result<Option<Description<C>>> {
        if let Some(node) = preferred {
            let device = EnabledNode::new(node, None);
            if let Some(description) = self.select_node(platform, device)? {
                return Ok(Some(description));
            }
        }
        let mut found = None;
        try_for_each_enabled_node(platform.root(), &mut |device, _| {
            if found.is_none()
                && let Some(description) = self.select_node(platform, device)?
            {
                found = Some(description);
            }
            Ok(())
        })?;
        Ok(found)
    }

    /// Selects the first matching device in driver-table order.
    ///
    /// Reset intentionally keeps its class-priority policy: the driver table
    /// is the primary order, and a node's compatible order is only consulted
    /// within the driver being tried.
    pub(crate) fn bind_first_by_driver(
        &self,
        platform: &PlatformView<'_>,
        memory: &mut MemoryRegistry,
    ) -> Result<Option<C::Device>> {
        for driver in self.entries {
            let mut found = None;
            try_for_each_enabled_node(platform.root(), &mut |device, _| {
                if found.is_none()
                    && let Some(description) = select_driver_node(*driver, platform, device)?
                {
                    found = Some(description.bind(memory)?);
                }
                Ok(())
            })?;
            if found.is_some() {
                return Ok(found);
            }
        }
        Ok(None)
    }

    fn select_node<'a, 'tree>(
        &self,
        platform: &'a PlatformView<'tree>,
        device: EnabledNode<'a, 'tree>,
    ) -> Result<Option<Description<C>>> {
        let Some(compatibles) = device.compatible() else {
            return Ok(None);
        };
        for compatible in compatibles.all() {
            for driver in self.entries {
                if let Some(description) =
                    select_driver_compatible(*driver, platform, device, compatible)?
                {
                    return Ok(Some(description));
                }
            }
        }
        Ok(None)
    }
}

/// Tries one compatible string against one driver.
fn select_driver_compatible<'a, 'tree, C: Class>(
    driver: &'static dyn Driver<C>,
    platform: &'a PlatformView<'tree>,
    device: EnabledNode<'a, 'tree>,
    compatible: &str,
) -> Result<Option<Description<C>>> {
    if !driver.compatibles().contains(&compatible) {
        return Ok(None);
    }
    driver.parse(platform, device, compatible)
}

/// Tries every compatible string of one node against one driver.
fn select_driver_node<'a, 'tree, C: Class>(
    driver: &'static dyn Driver<C>,
    platform: &'a PlatformView<'tree>,
    device: EnabledNode<'a, 'tree>,
) -> Result<Option<Description<C>>> {
    let Some(compatibles) = device.compatible() else {
        return Ok(None);
    };
    for compatible in compatibles.all() {
        if let Some(description) = select_driver_compatible(driver, platform, device, compatible)? {
            return Ok(Some(description));
        }
    }
    Ok(None)
}
