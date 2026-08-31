//! Arm PrimeCell UART (PL011).

use crate::driver::console::{ConsoleDevice, pl011_divisors};
use crate::platform::mmio::Mmio;

/// Register span through `UARTCR` at offset `0x30`.
pub(crate) const SPAN: usize = 0x34;

const FR_RX_FIFO_EMPTY: u32 = 1 << 4;
const FR_TX_FIFO_FULL: u32 = 1 << 5;
const DR_ERROR: u32 = 0xf << 8;
const LCR_H_8N1_FIFO: u32 = 0x60 | 1 << 4;
const CR_UARTEN_RXE_TXE: u32 = 1 << 0 | 1 << 8 | 1 << 9;
/// Input clock assumed by this driver; the FDT clock is not consulted.
const INPUT_CLOCK_HZ: u32 = 24_000_000;

#[derive(Clone, Copy)]
enum Reg {
    Data,
    StatusClear,
    Flags,
    IntBaud,
    FracBaud,
    LineControl,
    Control,
}

impl Reg {
    fn offset(self) -> usize {
        match self {
            Self::Data => 0x00,
            Self::StatusClear => 0x04,
            Self::Flags => 0x18,
            Self::IntBaud => 0x24,
            Self::FracBaud => 0x28,
            Self::LineControl => 0x2c,
            Self::Control => 0x30,
        }
    }
}

pub(super) struct UartPl011 {
    mmio: Mmio,
}

impl UartPl011 {
    /// Configures 115200 baud, 8N1, for a 24 MHz input clock; returns
    /// `None` when the fixed clock yields no representable divisor.
    pub(super) fn new(mmio: Mmio) -> Option<Self> {
        let (integer_divisor, fractional_divisor) = pl011_divisors(INPUT_CLOCK_HZ)?;
        mmio.write::<u32>(Reg::StatusClear.offset(), 0);
        mmio.write::<u32>(Reg::Control.offset(), 0);
        mmio.write::<u32>(Reg::IntBaud.offset(), u32::from(integer_divisor));
        mmio.write::<u32>(Reg::FracBaud.offset(), u32::from(fractional_divisor));
        mmio.write::<u32>(Reg::LineControl.offset(), LCR_H_8N1_FIFO);
        mmio.write::<u32>(Reg::Control.offset(), CR_UARTEN_RXE_TXE);
        Some(Self { mmio })
    }

    fn flags(&self) -> u32 {
        self.mmio.read::<u32>(Reg::Flags.offset())
    }

    fn read_byte(&self) -> Option<u8> {
        if self.flags() & FR_RX_FIFO_EMPTY != 0 {
            return None;
        }
        let data = self.mmio.read::<u32>(Reg::Data.offset());
        (data & DR_ERROR == 0).then_some(data as u8)
    }
}

impl ConsoleDevice for UartPl011 {
    fn read(&self, buf: &mut [u8]) -> usize {
        let mut count = 0;
        for slot in buf.iter_mut() {
            let Some(byte) = self.read_byte() else {
                break;
            };
            *slot = byte;
            count += 1;
        }
        count
    }

    fn write(&self, buf: &[u8]) -> usize {
        let mut count = 0;
        for &byte in buf {
            if self.flags() & FR_TX_FIFO_FULL != 0 {
                break;
            }
            self.mmio.write::<u32>(Reg::Data.offset(), byte as u32);
            count += 1;
        }
        count
    }
}
