//! USB DMA word-address bypass used by the V821 vendor SBI extension.
//!
//! Vendor register definitions: [V821 USB controller](https://github.com/sam-yangjj/tina-v821-v1.3-bsp/blob/4f82c3bed72342d306e1b14755f9618a6ca7a504/drivers/usb/sunxi_usb/include/sunxi_usb_bsp.h)
//! define the DMA word-address bypass register offset.

use runtime::memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion};

/// The USB controller's DMA word-address bypass register.
pub(crate) struct UsbDmaBypass {
    registers: MmioRegion,
}

impl UsbDmaBypass {
    pub(crate) fn bind(
        registers: DeviceRegisterRange,
        memory: &mut MemoryRegistry,
    ) -> runtime::Result<Self> {
        Ok(Self {
            registers: memory.acquire_mmio(registers)?,
        })
    }

    /// Enables DMA word-address bypass after the supervisor enables USB clocks.
    ///
    /// Concurrent calls are idempotent: each writes the same enable value.
    pub(crate) fn enable(&self) -> runtime::Result<()> {
        self.registers.synchronize();
        self.registers.write(0, 1u32)?;
        self.registers.synchronize();
        Ok(())
    }
}
