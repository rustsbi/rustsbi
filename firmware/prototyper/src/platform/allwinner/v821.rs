//! One-time V821 platform preparation.
//!
//! [`Description`] keeps V821-only discovery out of generic platform state.
//! [`V821`] proves that the boot contract and machine-wide PMA setup ran
//! before its [`ExtensionDeviceDescription`] can be bound.

#![forbid(unsafe_code)]

use runtime::memory::DeviceRegisterRange;
use runtime::soc::allwinner::v821::{AllwinnerV821Soc, NoncacheableAlias};

use crate::devicetree::EnabledNode;

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
    /// Creates a V821 description ready for the shared discovery pass.
    pub(crate) const fn new(soc: AllwinnerV821Soc) -> Self {
        Self {
            soc,
            cache: None,
            usb_dma_bypass: None,
        }
    }

    /// Retains V821-only device descriptions inside the vendor description.
    pub(crate) fn probe(
        &mut self,
        platform: &runtime::PlatformView<'_>,
        discovered: EnabledNode<'_, '_>,
    ) -> runtime::Result<()> {
        let Some(compatibles) = discovered.compatible() else {
            return Ok(());
        };
        let node = discovered.node();
        let is_l2 = compatibles.all().any(|value| value == L2_COMPATIBLE)
            && crate::devicetree::u32_property(node, "cache-level") == Some(2);
        let is_usb = compatibles.all().any(|value| value == USB_COMPATIBLE);
        if !is_l2 && !is_usb {
            return Ok(());
        }

        let registers = platform
            .device_register(node)?
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
