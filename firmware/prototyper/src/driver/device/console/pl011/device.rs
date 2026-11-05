//! Arm PL011 UART hardware device.

use bitflags::bitflags;
use core::mem::size_of;
use runtime::{
    Error, Result,
    console::ConsoleDevice,
    memory::{MemoryRegistry, MmioRegion},
};

use super::description::Desc;

const BAUD_RATE: u32 = 115_200;
// PL011 derives Baud16 from UARTCLK / 16.
// The fractional divisor is stored in sixty-fourths.
const CLOCK_OVERSAMPLING: u32 = 16;
const FRACTIONAL_SCALE: u32 = 64;

/// PL011 register offsets.
#[repr(usize)]
#[derive(Clone, Copy)]
enum Register {
    Data = 0x00,
    StatusClear = 0x04,
    Flags = 0x18,
    IntBaud = 0x24,
    FracBaud = 0x28,
    LineControl = 0x2c,
    Control = 0x30,
}

impl Register {
    const fn offset(self) -> usize {
        self as usize
    }
}

/// Minimum PL011 register-window size required by this driver.
pub(super) const SPAN: usize = Register::Control.offset() + size_of::<u32>();

bitflags! {
    /// PL011 flag bits used for FIFO readiness.
    struct Flags: u32 {
        const RX_FIFO_EMPTY = 1 << 4;
        const TX_FIFO_FULL = 1 << 5;
    }

    /// PL011 receive error bits reported with data.
    struct DataStatus: u32 {
        const FRAMING_ERROR = 1 << 8;
        const PARITY_ERROR = 1 << 9;
        const BREAK_ERROR = 1 << 10;
        const OVERRUN_ERROR = 1 << 11;
    }

    /// PL011 line-control fields for 8-bit words and the FIFO.
    struct LineControl: u32 {
        const FIFO_ENABLE = 1 << 4;
        const WORD_LENGTH_8 = 0b11 << 5;
    }

    /// PL011 UART and signal-enable bits.
    struct Control: u32 {
        const UART_ENABLE = 1 << 0;
        const TX_ENABLE = 1 << 8;
        const RX_ENABLE = 1 << 9;
    }
}

/// Integer and fractional PL011 baud divisors.
struct BaudDivisors {
    integer: u16,
    fractional: u8,
}

impl BaudDivisors {
    /// Returns the divisors for `clock_hz`, rounded to the nearest fractional step.
    fn from_clock_hz(clock_hz: u32) -> Option<Self> {
        let denominator = BAUD_RATE * CLOCK_OVERSAMPLING;
        let mut integer = clock_hz / denominator;
        let remainder = clock_hz % denominator;
        let mut fractional = (remainder * FRACTIONAL_SCALE + denominator / 2) / denominator;

        if fractional == FRACTIONAL_SCALE {
            integer += 1;
            fractional = 0;
        }
        let integer = u16::try_from(integer).ok()?;
        if integer == 0 {
            return None;
        }
        Some(Self {
            integer,
            fractional: fractional as u8,
        })
    }
}

/// An Arm PL011 UART bound to an acquired register window.
pub(super) struct Device {
    registers: MmioRegion,
}

impl Device {
    /// Configures 115200 baud and 8N1 using validated divisors.
    pub(super) fn init(description: Desc, memory: &mut MemoryRegistry) -> Result<Self> {
        let Desc {
            registers,
            clock_hz,
        } = description;
        let clock_hz = clock_hz.ok_or(Error::InvalidArgs)?;
        let divisors = BaudDivisors::from_clock_hz(clock_hz).ok_or(Error::InvalidArgs)?;
        let uart = Self {
            registers: memory.acquire(registers)?,
        };
        uart.write_reg(Register::StatusClear, 0);
        uart.write_reg(Register::Control, 0);
        uart.write_reg(Register::IntBaud, u32::from(divisors.integer));
        uart.write_reg(Register::FracBaud, u32::from(divisors.fractional));
        uart.write_reg(
            Register::LineControl,
            (LineControl::WORD_LENGTH_8 | LineControl::FIFO_ENABLE).bits(),
        );
        uart.write_reg(
            Register::Control,
            (Control::UART_ENABLE | Control::TX_ENABLE | Control::RX_ENABLE).bits(),
        );
        Ok(uart)
    }

    fn read_reg(&self, reg: Register) -> u32 {
        self.registers
            .read(reg.offset())
            .expect("BUG: PL011 register escaped its MMIO window")
    }

    fn write_reg(&self, reg: Register, value: u32) {
        self.registers
            .write(reg.offset(), value)
            .expect("BUG: PL011 register escaped its MMIO window")
    }

    fn flags(&self) -> Flags {
        Flags::from_bits_retain(self.read_reg(Register::Flags))
    }

    fn read_byte(&self) -> Result<Option<u8>> {
        if self.flags().contains(Flags::RX_FIFO_EMPTY) {
            return Ok(None);
        }
        let data_register = self.read_reg(Register::Data);
        let errors = DataStatus::from_bits_truncate(data_register);
        if !errors.is_empty() {
            return Err(Error::IoError);
        }
        Ok(Some(data_register as u8))
    }
}

impl ConsoleDevice for Device {
    fn read_slice(&mut self, buf: &mut [u8]) -> Result<usize> {
        let mut count = 0;
        for byte in buf.iter_mut() {
            let Some(received_byte) = self.read_byte()? else {
                break;
            };
            *byte = received_byte;
            count += 1;
        }
        Ok(count)
    }

    fn write_slice(&mut self, buf: &[u8]) -> Result<usize> {
        let mut count = 0;
        for &byte in buf {
            if self.flags().contains(Flags::TX_FIFO_FULL) {
                break;
            }
            self.write_reg(Register::Data, byte as u32);
            count += 1;
        }
        Ok(count)
    }
}
