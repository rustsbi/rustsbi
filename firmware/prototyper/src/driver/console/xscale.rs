//! Intel XScale/PXA UART.

use crate::driver::console::{ConsoleDevice, uart_divisor};
use crate::platform::mmio::Mmio;

/// Register span through the line status register at offset `0x14`.
pub(crate) const SPAN: usize = 0x18;

/// XScale `UUE`: the UART unit runs only while this IER bit is set.
const IER_UUE: u32 = 1 << 6;
const LCR_DLAB: u32 = 1 << 7;
const LCR_8N1: u32 = 0x03;
const FCR_ENABLE: u32 = 0x07;
const LSR_DATA_READY: u8 = 1 << 0;
const LSR_THR_EMPTY: u8 = 1 << 5;
const STRIDE: usize = 4;
const DEFAULT_INPUT_CLOCK_HZ: u32 = 14_857_000;

#[derive(Clone, Copy)]
enum Reg {
    RbrThrDll = 0,
    IerDlh = 1,
    FcrIir = 2,
    Lcr = 3,
    Lsr = 5,
}

impl Reg {
    fn offset(self) -> usize {
        STRIDE * self as usize
    }
}

pub(super) struct UartXscale {
    mmio: Mmio,
}

impl UartXscale {
    /// Enables 8N1 operation and sets 115200 baud, preserving the legacy
    /// 14,857,000 Hz input-clock fallback when the FDT omits one.
    pub(super) fn new(mmio: Mmio, clock_hz: Option<u32>) -> Option<Self> {
        // Clear DLAB before accessing IER; firmware may inherit any prior UART state.
        mmio.write::<u32>(Reg::Lcr.offset(), LCR_8N1);
        mmio.write::<u32>(Reg::IerDlh.offset(), IER_UUE);

        let divisor = u32::from(uart_divisor(clock_hz.unwrap_or(DEFAULT_INPUT_CLOCK_HZ))?);
        mmio.write::<u32>(Reg::Lcr.offset(), LCR_DLAB | LCR_8N1);
        mmio.write::<u32>(Reg::RbrThrDll.offset(), divisor & 0xff);
        mmio.write::<u32>(Reg::IerDlh.offset(), (divisor >> 8) & 0xff);
        mmio.write::<u32>(Reg::Lcr.offset(), LCR_8N1);

        mmio.write::<u32>(Reg::FcrIir.offset(), FCR_ENABLE);
        mmio.write::<u32>(Reg::IerDlh.offset(), IER_UUE);
        Some(Self { mmio })
    }

    fn line_status(&self) -> u8 {
        self.mmio.read::<u32>(Reg::Lsr.offset()) as u8
    }
}

impl ConsoleDevice for UartXscale {
    fn read(&self, buf: &mut [u8]) -> usize {
        let mut count = 0;
        for slot in buf.iter_mut() {
            if self.line_status() & LSR_DATA_READY == 0 {
                break;
            }
            *slot = self.mmio.read::<u32>(Reg::RbrThrDll.offset()) as u8;
            count += 1;
        }
        count
    }

    fn write(&self, buf: &[u8]) -> usize {
        for &byte in buf {
            while self.line_status() & LSR_THR_EMPTY == 0 {
                core::hint::spin_loop();
            }
            self.mmio.write::<u32>(Reg::RbrThrDll.offset(), byte as u32);
        }
        buf.len()
    }
}
