//! SiFive UART.

use crate::driver::console::ConsoleDevice;
use crate::platform::mmio::Mmio;

/// Register-block span covering the divisor register at offset 0x18.
pub(crate) const SPAN: usize = 28;

/// Control register enable bit (transmit and receive alike).
const CONTROL_ENABLE: u32 = 1 << 0;
/// Data register flag bit marking an empty receive / full transmit FIFO.
const FIFO_FLAG: u32 = 1 << 31;

/// Register offsets within the SiFive UART register map.
#[derive(Clone, Copy)]
enum Reg {
    /// Transmit data.
    TxData,
    /// Receive data.
    RxData,
    /// Transmit control (watermark and enable).
    TxControl,
    /// Receive control (watermark and enable).
    RxControl,
    /// Interrupt enable.
    InterruptEnable,
}

impl Reg {
    /// Byte offset of this register.
    fn offset(self) -> usize {
        4 * self as usize
    }
}

pub(super) struct UartSifive {
    mmio: Mmio,
}

impl UartSifive {
    /// Wraps an acquired register block and enables the transmit and
    /// receive channels with interrupts off.
    pub(super) fn new(mmio: Mmio) -> Self {
        mmio.write::<u32>(Reg::InterruptEnable.offset(), 0);
        let rxctrl = mmio.read::<u32>(Reg::RxControl.offset()) | CONTROL_ENABLE;
        mmio.write::<u32>(Reg::RxControl.offset(), rxctrl);
        let txctrl = mmio.read::<u32>(Reg::TxControl.offset()) | CONTROL_ENABLE;
        mmio.write::<u32>(Reg::TxControl.offset(), txctrl);
        Self { mmio }
    }

    /// Reads one received byte, or `None` when the receive FIFO is empty.
    fn read_byte(&self) -> Option<u8> {
        let rx = self.mmio.read::<u32>(Reg::RxData.offset());
        (rx & FIFO_FLAG == 0).then_some(rx as u8)
    }

    /// Reports whether the transmit FIFO is full.
    fn tx_fifo_full(&self) -> bool {
        self.mmio.read::<u32>(Reg::TxData.offset()) & FIFO_FLAG != 0
    }
}

impl ConsoleDevice for UartSifive {
    fn read(&self, buf: &mut [u8]) -> usize {
        let mut count = 0;
        for slot in buf.iter_mut() {
            match self.read_byte() {
                Some(byte) => {
                    *slot = byte;
                    count += 1;
                }
                None => break,
            }
        }
        count
    }

    fn write(&self, buf: &[u8]) -> usize {
        let mut count = 0;
        for &byte in buf {
            if self.tx_fifo_full() {
                break;
            }
            self.mmio.write::<u32>(Reg::TxData.offset(), byte as u32);
            count += 1;
        }
        count
    }
}
