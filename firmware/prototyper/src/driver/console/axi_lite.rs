//! Xilinx AXI UART Lite.
//!
//! Register definitions follow
//! [Xilinx DS741, *LogiCORE IP AXI UART Lite*](https://docs.amd.com/api/khub/documents/86Ejvsef_6A3PXWuJfH6SA/content).

use crate::driver::console::ConsoleDevice;
use crate::platform::mmio::Mmio;

/// Register-block span covering the control register at offset 0xc.
pub(crate) const SPAN: usize = 16;

/// Status register bits.
const STATUS_RX_VALID: u32 = 1 << 0;
const STATUS_TX_FULL: u32 = 1 << 3;

/// Register offsets within the UART Lite register map.
#[derive(Clone, Copy)]
enum Reg {
    /// Receive FIFO data.
    Rx,
    /// Transmit FIFO data.
    Tx,
    /// Status flags.
    Status,
}

impl Reg {
    /// Byte offset of this register.
    fn offset(self) -> usize {
        4 * self as usize
    }
}

pub(super) struct UartAxiLite {
    mmio: Mmio,
}

impl UartAxiLite {
    /// Wraps an acquired register block.
    pub(super) fn new(mmio: Mmio) -> Self {
        Self { mmio }
    }

    fn status(&self) -> u32 {
        self.mmio.read::<u32>(Reg::Status.offset())
    }
}

impl ConsoleDevice for UartAxiLite {
    fn read(&self, buf: &mut [u8]) -> usize {
        let mut count = 0;
        for slot in buf.iter_mut() {
            if self.status() & STATUS_RX_VALID == 0 {
                break;
            }
            *slot = self.mmio.read::<u32>(Reg::Rx.offset()) as u8;
            count += 1;
        }
        count
    }

    fn write(&self, buf: &[u8]) -> usize {
        let mut count = 0;
        for &byte in buf {
            if self.status() & STATUS_TX_FULL != 0 {
                break;
            }
            self.mmio.write::<u32>(Reg::Tx.offset(), byte as u32);
            count += 1;
        }
        count
    }
}
