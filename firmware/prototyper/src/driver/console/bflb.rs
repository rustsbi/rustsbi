//! Bouffalo Lab BL808 UART.

use crate::driver::console::ConsoleDevice;
use crate::platform::mmio::Mmio;

/// Register-block span covering the FIFO read byte at offset 0x8c.
pub(crate) const SPAN: usize = 0x90;

/// FIFO configuration register field masks.
const TX_AVAILABLE: u32 = 0x3f;
const RX_AVAILABLE: u32 = 0x3f << 8;

/// Register offsets within the BL808 UART register map.
#[derive(Clone, Copy)]
enum Reg {
    /// FIFO configuration 1 (fill counters and thresholds).
    FifoConfig1,
    /// FIFO write data.
    FifoWrite,
    /// FIFO read data.
    FifoRead,
}

impl Reg {
    /// Byte offset of this register.
    fn offset(self) -> usize {
        match self {
            Reg::FifoConfig1 => 0x84,
            Reg::FifoWrite => 0x88,
            Reg::FifoRead => 0x8c,
        }
    }
}

pub(super) struct UartBflb {
    mmio: Mmio,
}

impl UartBflb {
    /// Wraps an acquired register block.
    pub(super) fn new(mmio: Mmio) -> Self {
        Self { mmio }
    }

    fn fifo_config_1(&self) -> u32 {
        self.mmio.read::<u32>(Reg::FifoConfig1.offset())
    }
}

impl ConsoleDevice for UartBflb {
    fn read(&self, buf: &mut [u8]) -> usize {
        let rx_available = ((self.fifo_config_1() & RX_AVAILABLE) >> 8) as usize;
        if rx_available == 0 {
            return 0;
        }
        let len = core::cmp::min(rx_available, buf.len());
        buf.iter_mut()
            .take(len)
            .for_each(|slot| *slot = self.mmio.read::<u8>(Reg::FifoRead.offset()));
        len
    }

    fn write(&self, buf: &[u8]) -> usize {
        let mut count = 0;
        for &byte in buf {
            if self.fifo_config_1() & TX_AVAILABLE == 0 {
                break;
            }
            count += 1;
            self.mmio.write::<u8>(Reg::FifoWrite.offset(), byte);
        }
        count
    }
}
