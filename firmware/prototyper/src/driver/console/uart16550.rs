//! NS16550-compatible UART with byte- or word-spaced registers.

use crate::driver::console::ConsoleDevice;
use crate::platform::mmio::Mmio;

/// Span through the byte-wide line status register.
pub(crate) const BYTE_SPAN: usize = 6;
/// Span through the word-wide line status register.
pub(crate) const WORD_SPAN: usize = 24;

const LSR_DATA_READY: u8 = 1 << 0;
const LSR_THR_EMPTY: u8 = 1 << 5;

#[derive(Clone, Copy)]
enum Reg {
    RbrThr = 0,
    Lsr = 5,
}

impl Reg {
    fn offset(self, stride: usize) -> usize {
        stride * self as usize
    }
}

pub(super) struct Uart16550 {
    mmio: Mmio,
    stride: usize,
}

impl Uart16550 {
    /// Wraps an acquired register block; `word_wide` selects the 4-byte
    /// register stride of word-spaced devices.
    pub(super) fn new(mmio: Mmio, word_wide: bool) -> Self {
        Self {
            mmio,
            stride: if word_wide { 4 } else { 1 },
        }
    }

    fn line_status(&self) -> u8 {
        if self.stride == 4 {
            self.mmio.read::<u32>(Reg::Lsr.offset(self.stride)) as u8
        } else {
            self.mmio.read::<u8>(Reg::Lsr.offset(self.stride))
        }
    }

    fn read_byte(&self) -> u8 {
        if self.stride == 4 {
            self.mmio.read::<u32>(Reg::RbrThr.offset(self.stride)) as u8
        } else {
            self.mmio.read::<u8>(Reg::RbrThr.offset(self.stride))
        }
    }

    fn write_byte(&self, value: u8) {
        let offset = Reg::RbrThr.offset(self.stride);
        if self.stride == 4 {
            self.mmio.write::<u32>(offset, value as u32);
        } else {
            self.mmio.write::<u8>(offset, value);
        }
    }
}

impl ConsoleDevice for Uart16550 {
    fn read(&self, buf: &mut [u8]) -> usize {
        let mut count = 0;
        for slot in buf.iter_mut() {
            if self.line_status() & LSR_DATA_READY == 0 {
                break;
            }
            *slot = self.read_byte();
            count += 1;
        }
        count
    }

    fn write(&self, buf: &[u8]) -> usize {
        let mut count = 0;
        for &byte in buf {
            if self.line_status() & LSR_THR_EMPTY == 0 {
                break;
            }
            self.write_byte(byte);
            count += 1;
        }
        count
    }
}
