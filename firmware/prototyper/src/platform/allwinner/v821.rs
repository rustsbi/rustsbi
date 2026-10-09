//! One-time V821 platform preparation.
//!
//! [`Description`] keeps V821-only discovery out of generic platform state.
//! [`V821`] proves that the boot contract, PMA, SRAM handshake, and selected
//! clock setup completed before devices can be bound.

use runtime::memory::{DeviceRegisterRange, MemoryRegistry};
use runtime::soc::allwinner::v821::{AllwinnerV821Soc, NoncacheableAlias};

use crate::devicetree::EnabledNode;
use crate::driver::allwinner::v821::{self, A27L2Cache, UsbDmaBypass};
use crate::platform::error::{self, ResultContext};

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

    /// Validates the boot contract and prepares PMA and boot devices before binding.
    pub(in crate::platform) fn prepare(
        self,
        memory: &mut MemoryRegistry,
        enable_plmt_clock: bool,
    ) -> error::Result<V821> {
        let hart = runtime::hart::HartId::current()
            .map_err(|_| runtime::Error::InvalidArgs)
            .during("preparing V821 platform resources")?;
        const BOOT_HART: usize = 0;
        const ANDES_VENDOR_ID: usize = 0x31e;
        if hart.as_usize() != BOOT_HART
            || runtime::hart::HartId::count() != 1
            || runtime::hart::vendor_id() != ANDES_VENDOR_ID
        {
            return Err(runtime::Error::InvalidArgs).during("preparing V821 platform resources");
        }
        let noncacheable_alias = self.soc.initialize_noncacheable_alias();
        v821::release_boot0_isp_sram(self.soc, memory).during("releasing V821 boot0 ISP SRAM")?;
        if enable_plmt_clock {
            v821::enable_plmt_clock(self.soc, memory).during("enabling the V821 PLMT clock")?;
        }
        Ok(V821 {
            description: self,
            noncacheable_alias,
        })
    }
}

/// A V821 SoC whose boot contract, PMA, and boot devices are prepared.
pub(crate) struct V821 {
    description: Description,
    noncacheable_alias: NoncacheableAlias,
}

impl V821 {
    /// Returns the offset of the noncacheable alias established during preparation.
    pub(crate) const fn noncacheable_alias_offset(&self) -> u64 {
        self.noncacheable_alias.offset()
    }

    /// Binds the devices consumed by V821 custom extensions after preparation.
    pub(crate) fn bind_extension_devices(
        self,
        memory: &mut MemoryRegistry,
    ) -> runtime::Result<(A27L2Cache, Option<UsbDmaBypass>)> {
        let cache = self.description.cache.ok_or(runtime::Error::InvalidArgs)?;
        let cache = A27L2Cache::bind(self.description.soc, cache, memory)?;
        let usb_dma_bypass = self
            .description
            .usb_dma_bypass
            .map(|registers| UsbDmaBypass::bind(registers, memory))
            .transpose()?;
        Ok((cache, usb_dma_bypass))
    }
}
