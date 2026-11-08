//! V821 USB DMA word-address bypass register.
//!
//! Register definition: [V821 USB controller](https://github.com/sam-yangjj/tina-v821-v1.3-bsp/blob/4f82c3bed72342d306e1b14755f9618a6ca7a504/drivers/usb/sunxi_usb/include/sunxi_usb_bsp.h).

use runtime::{
    Result,
    memory::{MemoryRegistry, MmioRegion},
};

use super::description::Desc;

pub(crate) struct UsbDmaBypass {
    registers: MmioRegion,
}

impl UsbDmaBypass {
    pub(crate) fn init(description: Desc, memory: &mut MemoryRegistry) -> Result<Self> {
        Ok(Self {
            registers: memory.acquire(description.registers)?,
        })
    }

    /// Enables DMA word-address bypass after the supervisor enables USB clocks.
    ///
    /// Concurrent calls write the same idempotent value.
    pub(crate) fn enable(&self) -> Result<()> {
        runtime::instruction::fence();
        self.registers.write(0, 1u32)?;
        runtime::instruction::fence();
        Ok(())
    }
}
