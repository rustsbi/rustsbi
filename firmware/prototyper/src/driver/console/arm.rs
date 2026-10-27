//! Arm PrimeCell UART (PL011).
//!
//! # References
//!
//! - Hardware manual: [Arm DDI 0183G, *PrimeCell UART (PL011) Technical Reference Manual*](https://developer.arm.com/documentation/ddi0183/g/),
//!   Programmer's Model — register layout, fields, and baud-rate divisors.

use alloc::boxed::Box;
use bitflags::bitflags;
use core::mem::size_of;
use runtime::{
    Error, PlatformView, Result,
    memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion},
};

use crate::devicetree::{EnabledNode, u32_property};
use crate::driver::console::{
    BAUD_RATE, Console, ConsoleDevice, ConsoleError, Description, Driver, acquire_registers,
};

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

fn read_reg(registers: &MmioRegion, reg: Register) -> u32 {
    registers
        .read(reg.offset())
        .expect("BUG: PL011 register escaped its MMIO window")
}

fn write_reg(registers: &MmioRegion, reg: Register, value: u32) {
    registers
        .write(reg.offset(), value)
        .expect("BUG: PL011 register escaped its MMIO window")
}

fn flags(registers: &MmioRegion) -> Flags {
    Flags::from_bits_retain(read_reg(registers, Register::Flags))
}

fn read_byte(registers: &MmioRegion) -> Result<Option<u8>, ConsoleError> {
    if flags(registers).contains(Flags::RX_FIFO_EMPTY) {
        return Ok(None);
    }
    let data_register = read_reg(registers, Register::Data);
    let errors = DataStatus::from_bits_truncate(data_register);
    if !errors.is_empty() {
        return Err(ConsoleError::Failed);
    }
    Ok(Some(data_register as u8))
}

fn init(registers: &MmioRegion, clock_hz: Option<u32>) -> Result<()> {
    let clock_hz = clock_hz.ok_or(Error::InvalidArgs)?;
    let divisors = BaudDivisors::from_clock_hz(clock_hz).ok_or(Error::InvalidArgs)?;
    write_reg(registers, Register::StatusClear, 0);
    write_reg(registers, Register::Control, 0);
    write_reg(registers, Register::IntBaud, u32::from(divisors.integer));
    write_reg(
        registers,
        Register::FracBaud,
        u32::from(divisors.fractional),
    );
    write_reg(
        registers,
        Register::LineControl,
        (LineControl::WORD_LENGTH_8 | LineControl::FIFO_ENABLE).bits(),
    );
    write_reg(
        registers,
        Register::Control,
        (Control::UART_ENABLE | Control::TX_ENABLE | Control::RX_ENABLE).bits(),
    );
    Ok(())
}

fn write(registers: &MmioRegion, src: &[u8]) -> Result<usize, ConsoleError> {
    let mut count = 0;
    for &byte in src {
        if flags(registers).contains(Flags::TX_FIFO_FULL) {
            break;
        }
        write_reg(registers, Register::Data, byte as u32);
        count += 1;
    }
    Ok(count)
}

fn read(registers: &MmioRegion, dst: &mut [u8]) -> Result<usize, ConsoleError> {
    let mut count = 0;
    for byte in dst.iter_mut() {
        let Some(received_byte) = read_byte(registers)? else {
            break;
        };
        *byte = received_byte;
        count += 1;
    }
    Ok(count)
}

struct Pl011 {
    registers: MmioRegion,
}

impl ConsoleDevice for Pl011 {
    fn write_slice(&mut self, src: &[u8]) -> Result<usize, ConsoleError> {
        write(&self.registers, src)
    }

    fn read_slice(&mut self, dst: &mut [u8]) -> Result<usize, ConsoleError> {
        read(&self.registers, dst)
    }
}

fn bind(
    registers: DeviceRegisterRange,
    clock_hz: Option<u32>,
    memory: &mut MemoryRegistry,
) -> Result<Box<dyn ConsoleDevice>> {
    let registers = acquire_registers::<u32>(registers, SPAN, memory)?;
    init(&registers, clock_hz)?;
    Ok(Box::new(Pl011 { registers }))
}

/// Driver for Arm PrimeCell PL011 UARTs.
pub(super) struct Pl011Driver;

pub(super) static PL011: Pl011Driver = Pl011Driver;

impl Driver<Console> for Pl011Driver {
    fn compatibles(&self) -> &'static [&'static str] {
        &["pl011", "arm,pl011"]
    }

    fn parse<'a, 'tree>(
        &self,
        platform: &'a PlatformView<'tree>,
        device: EnabledNode<'a, 'tree>,
        _compatible: &str,
    ) -> Result<Option<Description>> {
        let registers = platform
            .device_register(device.node())?
            .ok_or(Error::InvalidArgs)?;
        let clock_hz = u32_property(device.node(), "clock-frequency");
        Ok(Some(Description::new("pl011", registers, move |memory| {
            bind(registers, clock_hz, memory)
        })))
    }
}
