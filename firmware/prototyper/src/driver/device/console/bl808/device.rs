//! Bouffalo Lab BL808 UART hardware device.

use core::mem::size_of;
use runtime::{
    Result,
    console::ConsoleDevice,
    memory::{MemoryRegistry, MmioRegion},
};

use super::description::Desc;

/// BL808 UART register offsets.
#[repr(usize)]
#[derive(Clone, Copy)]
enum Register {
    FifoConfig1 = 0x84,
    TxData = 0x88,
    RxData = 0x8c,
}

impl Register {
    const fn offset(self) -> usize {
        self as usize
    }
}

/// Minimum BL808 UART register-window size required by this driver.
pub(super) const SPAN: usize = Register::RxData.offset() + size_of::<u32>();

/// Encoded transmit and receive FIFO counts from the FIFO configuration register.
struct FifoCounts(u32);

impl FifoCounts {
    const COUNT_MASK: u32 = 0x3f;
    const RX_COUNT_SHIFT: u32 = 8;

    fn tx_available_count(&self) -> usize {
        (self.0 & Self::COUNT_MASK) as usize
    }

    fn rx_available_count(&self) -> usize {
        ((self.0 >> Self::RX_COUNT_SHIFT) & Self::COUNT_MASK) as usize
    }
}

/// A BL808 UART bound to an acquired register window.
pub(super) struct Device {
    registers: MmioRegion,
}

impl Device {
    /// Acquires the BL808 UART register window.
    pub(super) fn init(description: Desc, memory: &mut MemoryRegistry) -> Result<Self> {
        let Desc { registers } = description;
        Ok(Self {
            registers: memory.acquire(registers)?,
        })
    }

    fn fifo_counts(&self) -> FifoCounts {
        FifoCounts(
            self.registers
                .read(Register::FifoConfig1.offset())
                .expect("BUG: BL808 FIFO configuration register escaped its MMIO window"),
        )
    }

    fn read_u8(&self, reg: Register) -> u8 {
        self.registers
            .read(reg.offset())
            .expect("BUG: BL808 UART register escaped its MMIO window")
    }

    fn write_u8(&self, reg: Register, value: u8) {
        self.registers
            .write(reg.offset(), value)
            .expect("BUG: BL808 UART register escaped its MMIO window")
    }
}

impl ConsoleDevice for Device {
    fn read_slice(&mut self, buf: &mut [u8]) -> Result<usize> {
        let available_count = self.fifo_counts().rx_available_count();
        if available_count == 0 {
            return Ok(0);
        }
        let len = core::cmp::min(available_count, buf.len());
        buf.iter_mut()
            .take(len)
            .for_each(|byte| *byte = self.read_u8(Register::RxData));
        Ok(len)
    }

    fn write_slice(&mut self, buf: &[u8]) -> Result<usize> {
        let mut count = 0;
        for &byte in buf {
            if self.fifo_counts().tx_available_count() == 0 {
                break;
            }
            count += 1;
            self.write_u8(Register::TxData, byte);
        }
        Ok(count)
    }
}
