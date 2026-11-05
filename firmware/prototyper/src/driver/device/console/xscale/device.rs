//! Intel XScale/PXA UART hardware device.

use bitflags::bitflags;
use core::mem::size_of;
use runtime::{
    Error, Result,
    console::ConsoleDevice,
    memory::{MemoryRegistry, MmioRegion},
};

use super::description::Desc;

const BAUD_RATE: u32 = 115_200;
const CLOCK_OVERSAMPLING: u32 = 16;

/// XScale UART register offsets.
#[repr(usize)]
#[derive(Clone, Copy)]
enum Register {
    DataOrDivisorLow = 0x00,
    InterruptEnableOrDivisorHigh = 0x04,
    FifoControlOrInterruptId = 0x08,
    LineControl = 0x0c,
    LineStatus = 0x14,
}

impl Register {
    const fn offset(self) -> usize {
        self as usize
    }
}

/// Minimum XScale UART register-window size required by this driver.
pub(super) const SPAN: usize = Register::LineStatus.offset() + size_of::<u32>();

bitflags! {
    /// XScale interrupt-enable bits.
    struct InterruptEnable: u32 {
        /// XScale UART Unit Enable (UUE).
        const UART_UNIT_ENABLE = 1 << 6;
    }

    /// XScale line-control fields for 8-bit words and divisor access.
    struct LineControl: u32 {
        const WORD_LENGTH_8 = 0b11;
        const DIVISOR_LATCH_ACCESS = 1 << 7;
    }

    /// XScale FIFO control and reset bits.
    struct FifoControl: u32 {
        const FIFO_ENABLE = 1 << 0;
        const RX_FIFO_RESET = 1 << 1;
        const TX_FIFO_RESET = 1 << 2;
    }

    /// XScale line-status bits for received data and transmitter readiness.
    struct LineStatus: u32 {
        const DATA_READY = 1 << 0;
        const TX_HOLDING_REGISTER_EMPTY = 1 << 5;
    }
}

/// Computes the rounded 16x divisor, or `None` when the clock cannot represent one.
fn baud_divisor(clock_hz: u32) -> Option<u16> {
    let denominator = BAUD_RATE * CLOCK_OVERSAMPLING;
    let divisor = clock_hz.checked_add(denominator / 2)? / denominator;
    u16::try_from(divisor).ok().filter(|divisor| *divisor != 0)
}

/// An XScale UART bound to an acquired register window.
pub(super) struct Device {
    registers: MmioRegion,
}

impl Device {
    /// Enables 8N1 operation and applies the selected baud setup.
    pub(super) fn init(description: Desc, memory: &mut MemoryRegistry) -> Result<Self> {
        let Desc {
            registers,
            clock_hz,
        } = description;
        // Preserve the previous boot stage's divisor if no clock rate is supplied.
        let divisor = clock_hz
            .map(|clock_hz| baud_divisor(clock_hz).ok_or(Error::InvalidArgs))
            .transpose()?;
        let uart = Self {
            registers: memory.acquire(registers)?,
        };
        uart.configure(divisor);
        Ok(uart)
    }

    fn configure(&self, divisor: Option<u16>) {
        // Clear DLAB before accessing IER.
        // Firmware may inherit any prior UART state.
        self.write_reg(Register::LineControl, LineControl::WORD_LENGTH_8.bits());
        self.write_reg(
            Register::InterruptEnableOrDivisorHigh,
            InterruptEnable::UART_UNIT_ENABLE.bits(),
        );

        if let Some(divisor) = divisor {
            self.program_baud(divisor);
        }

        self.write_reg(
            Register::FifoControlOrInterruptId,
            (FifoControl::FIFO_ENABLE | FifoControl::RX_FIFO_RESET | FifoControl::TX_FIFO_RESET)
                .bits(),
        );
        self.write_reg(
            Register::InterruptEnableOrDivisorHigh,
            InterruptEnable::UART_UNIT_ENABLE.bits(),
        );
    }

    fn program_baud(&self, divisor: u16) {
        let [low, high] = divisor.to_le_bytes();
        self.write_reg(
            Register::LineControl,
            (LineControl::DIVISOR_LATCH_ACCESS | LineControl::WORD_LENGTH_8).bits(),
        );
        self.write_reg(Register::DataOrDivisorLow, u32::from(low));
        self.write_reg(Register::InterruptEnableOrDivisorHigh, u32::from(high));
        self.write_reg(Register::LineControl, LineControl::WORD_LENGTH_8.bits());
    }

    fn read_reg(&self, reg: Register) -> u32 {
        self.registers
            .read(reg.offset())
            .expect("BUG: XScale UART register escaped its MMIO window")
    }

    fn write_reg(&self, reg: Register, value: u32) {
        self.registers
            .write(reg.offset(), value)
            .expect("BUG: XScale UART register escaped its MMIO window")
    }

    fn line_status(&self) -> LineStatus {
        LineStatus::from_bits_retain(self.read_reg(Register::LineStatus))
    }
}

impl ConsoleDevice for Device {
    fn read_slice(&mut self, buf: &mut [u8]) -> Result<usize> {
        let mut count = 0;
        for byte in buf.iter_mut() {
            if !self.line_status().contains(LineStatus::DATA_READY) {
                break;
            }
            *byte = self.read_reg(Register::DataOrDivisorLow) as u8;
            count += 1;
        }
        Ok(count)
    }

    fn write_slice(&mut self, buf: &[u8]) -> Result<usize> {
        let mut count = 0;
        for &byte in buf {
            if !self
                .line_status()
                .contains(LineStatus::TX_HOLDING_REGISTER_EMPTY)
            {
                break;
            }
            self.write_reg(Register::DataOrDivisorLow, byte as u32);
            count += 1;
        }
        Ok(count)
    }
}
