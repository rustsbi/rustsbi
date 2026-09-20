//! One-time V821 platform preparation.
//!
//! [`Description`] keeps V821-only discovery out of generic platform state.
//! [`V821`] proves that the boot contract and machine-wide PMA setup ran
//! before its [`ExtensionDeviceDescription`] can be bound.

#![forbid(unsafe_code)]

use runtime::memory::DeviceRegisterRange;
use runtime::soc::allwinner::v821::{AllwinnerV821Soc, NoncacheableAlias};
use serde_device_tree::buildin::Node;

use crate::devicetree;

const L2_COMPATIBLE: &str = "cache";
const USB_COMPATIBLE: &str = "allwinner,sunxi-udc";
const USB_DMA_BYPASS_OFFSET: usize = 0x5c0;
const USB_DMA_BYPASS_SIZE: usize = 4;

/// V821 resources collected from the Platform Description before binding.
pub(crate) struct Description {
    soc: AllwinnerV821Soc,
    cache: Option<DeviceRegisterRange>,
    usb_dma_bypass: Option<DeviceRegisterRange>,
}

/// Prepared descriptions of devices used only by V821 custom SBI extensions.
pub(crate) struct ExtensionDeviceDescription {
    soc: AllwinnerV821Soc,
    cache: DeviceRegisterRange,
    usb_dma_bypass: Option<DeviceRegisterRange>,
}

impl Description {
    pub(crate) fn discover(
        soc: AllwinnerV821Soc,
        platform: &runtime::PlatformView<'_>,
    ) -> runtime::Result<Self> {
        let mut description = Self {
            soc,
            cache: None,
            usb_dma_bypass: None,
        };
        description.visit(platform, platform.root())?;
        Ok(description)
    }

    fn visit<'tree>(
        &mut self,
        platform: &runtime::PlatformView<'tree>,
        node: &Node<'tree>,
    ) -> runtime::Result<()> {
        if !runtime::node_is_enabled(node) {
            return Ok(());
        }
        self.probe(platform, node)?;
        for child in node.nodes() {
            let child = child.deserialize::<Node<'tree>>();
            self.visit(platform, &child)?;
        }
        Ok(())
    }

    /// Retains V821-only device descriptions inside the vendor description.
    fn probe(
        &mut self,
        platform: &runtime::PlatformView<'_>,
        node: &Node<'_>,
    ) -> runtime::Result<()> {
        let Some(compatibles) = devicetree::compatible_strings(node) else {
            return Ok(());
        };
        let is_l2 = compatibles.iter().any(|value| value == L2_COMPATIBLE)
            && node
                .get_prop("cache-level")
                .is_some_and(|property| property.deserialize::<u32>() == 2);
        let is_usb = compatibles.iter().any(|value| value == USB_COMPATIBLE);
        if !is_l2 && !is_usb {
            return Ok(());
        }

        let registers = platform
            .device_registers(node)?
            .and_then(|ranges| ranges.first().copied())
            .ok_or(runtime::Error::InvalidArgs)?;
        if is_l2 && self.cache.replace(registers).is_some() {
            return Err(runtime::Error::InvalidArgs);
        }
        if is_usb {
            let dma_bypass = registers.subrange(USB_DMA_BYPASS_OFFSET, USB_DMA_BYPASS_SIZE)?;
            if self.usb_dma_bypass.replace(dma_bypass).is_some() {
                return Err(runtime::Error::InvalidArgs);
            }
        }
        Ok(())
    }

    /// Validates the boot contract and prepares the machine-wide PMA state.
    pub(crate) fn prepare(self, hart_count: usize) -> runtime::Result<V821> {
        let hart = runtime::hart::HartId::current().map_err(|_| runtime::Error::InvalidArgs)?;
        const BOOT_HART: usize = 0;
        const ANDES_VENDOR_ID: usize = 0x31e;
        if hart.as_usize() != BOOT_HART
            || hart_count != 1
            || riscv::register::mvendorid::read().bits() != ANDES_VENDOR_ID
        {
            return Err(runtime::Error::InvalidArgs);
        }
        let noncacheable_alias = self.soc.csr().initialize_noncacheable_alias();
        Ok(V821 {
            description: self,
            noncacheable_alias,
        })
    }
}

/// A V821 SoC whose boot contract and machine-wide PMA are prepared.
pub(crate) struct V821 {
    description: Description,
    noncacheable_alias: NoncacheableAlias,
}

impl V821 {
    /// Returns the offset of the noncacheable alias established during preparation.
    pub(crate) const fn noncacheable_alias_offset(&self) -> u64 {
        self.noncacheable_alias.offset()
    }

    /// Returns the SoC capability required by V821 firmware drivers.
    pub(crate) const fn soc(&self) -> AllwinnerV821Soc {
        self.description.soc
    }

    /// Returns the prepared descriptions consumed by V821 custom extensions.
    pub(crate) fn into_extension_devices(self) -> runtime::Result<ExtensionDeviceDescription> {
        let cache = self.description.cache.ok_or(runtime::Error::InvalidArgs)?;
        Ok(ExtensionDeviceDescription {
            soc: self.description.soc,
            cache,
            usb_dma_bypass: self.description.usb_dma_bypass,
        })
    }
}

impl ExtensionDeviceDescription {
    /// Splits this prepared description for the custom-extension binder.
    pub(crate) fn into_parts(
        self,
    ) -> (
        AllwinnerV821Soc,
        DeviceRegisterRange,
        Option<DeviceRegisterRange>,
    ) {
        (self.soc, self.cache, self.usb_dma_bypass)
    }
}
