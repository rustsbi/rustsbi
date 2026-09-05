//! Arm PrimeCell UART (PL011).
//!
//! # References
//!
//! - Hardware manual: [Arm DDI 0183G, *PrimeCell UART (PL011) Technical Reference Manual*](https://developer.arm.com/documentation/ddi0183/g/),
//!   Programmer's Model — register layout, fields, and baud-rate divisors.

use alloc::boxed::Box;
use bitflags::bitflags;
use core::mem::size_of;
use runtime::memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion};

use crate::driver::console::{BAUD_RATE, ConsoleDevice, acquire_registers};

// PL011 derives Baud16 from UARTCLK / 16 and stores the fractional divisor in
// sixty-fourths.
const CLOCK_OVERSAMPLING: u32 = 16;
const FRACTIONAL_SCALE: u32 = 64;

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

const SPAN: usize = Register::Control.offset() + size_of::<u32>();

bitflags! {
    struct Flags: u32 {
        const RX_FIFO_EMPTY = 1 << 4;
        const TX_FIFO_FULL = 1 << 5;
    }

    struct DataStatus: u32 {
        const FRAMING_ERROR = 1 << 8;
        const PARITY_ERROR = 1 << 9;
        const BREAK_ERROR = 1 << 10;
        const OVERRUN_ERROR = 1 << 11;
    }

    struct LineControl: u32 {
        const FIFO_ENABLE = 1 << 4;
        const WORD_LENGTH_8 = 0b11 << 5;
    }

    struct Control: u32 {
        const UART_ENABLE = 1 << 0;
        const TX_ENABLE = 1 << 8;
        const RX_ENABLE = 1 << 9;
    }
}

pub(super) fn bind(
    registers: DeviceRegisterRange,
    clock_hz: Option<u32>,
    memory: &mut MemoryRegistry,
) -> runtime::Result<Box<dyn ConsoleDevice>> {
    let clock_hz = clock_hz.ok_or(runtime::Error::InvalidArgs)?;
    let divisors = BaudDivisors::from_clock_hz(clock_hz).ok_or(runtime::Error::InvalidArgs)?;
    let registers = acquire_registers::<u32>(registers, SPAN, memory)?;
    Ok(Box::new(UartPl011::new(registers, divisors)))
}

struct BaudDivisors {
    integer: u16,
    fractional: u8,
}

impl BaudDivisors {
    fn from_clock_hz(clock_hz: u32) -> Option<Self> {
        let denominator = BAUD_RATE.checked_mul(CLOCK_OVERSAMPLING)?;
        let mut integer = clock_hz / denominator;
        let remainder = clock_hz % denominator;
        let mut fractional = remainder
            .checked_mul(FRACTIONAL_SCALE)?
            .checked_add(denominator / 2)?
            / denominator;

        if fractional == FRACTIONAL_SCALE {
            integer = integer.checked_add(1)?;
            fractional = 0;
        }
        if !(1..=u16::MAX as u32).contains(&integer) || fractional >= FRACTIONAL_SCALE {
            return None;
        }
        Some(Self {
            integer: integer as u16,
            fractional: fractional as u8,
        })
    }
}

struct UartPl011 {
    registers: MmioRegion,
}

impl UartPl011 {
    /// Configures 115200 baud and 8N1 using validated divisors.
    fn new(registers: MmioRegion, divisors: BaudDivisors) -> Self {
        let uart = Self { registers };
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
        uart
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

    fn read_byte(&self) -> Option<u8> {
        if self.flags().contains(Flags::RX_FIFO_EMPTY) {
            return None;
        }
        let data_register = self.read_reg(Register::Data);
        let errors = DataStatus::from_bits_retain(data_register);
        errors.is_empty().then_some(data_register as u8)
    }
}

impl ConsoleDevice for UartPl011 {
    fn read(&self, buf: &mut [u8]) -> usize {
        let mut count = 0;
        for byte in buf.iter_mut() {
            let Some(received_byte) = self.read_byte() else {
                break;
            };
            *byte = received_byte;
            count += 1;
        }
        count
    }

    fn write(&self, buf: &[u8]) -> usize {
        let mut count = 0;
        for &byte in buf {
            if self.flags().contains(Flags::TX_FIFO_FULL) {
                break;
            }
            self.write_reg(Register::Data, byte as u32);
            count += 1;
        }
        count
    }
}
