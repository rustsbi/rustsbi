//! Intel XScale/PXA UART.
//!
//! # References
//!
//! - Hardware manual: [Intel PXA27x Processor Family Developer's Manual, order 280000-001](https://support.eurotech-inc.com/developers/datasheets/PXA270_DeveloperManual.pdf),
//!   Chapter 10 — full-function UART register layout and fields.

use alloc::boxed::Box;
use bitflags::bitflags;
use core::mem::size_of;
use runtime::{
    Error, PlatformView, Result,
    memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion},
};

use crate::devicetree::{EnabledNode, u32_property};
use crate::driver::console::{
    Console, ConsoleDevice, ConsoleError, Description, Driver, acquire_registers, uart_divisor,
};

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

const SPAN: usize = Register::LineStatus.offset() + size_of::<u32>();

bitflags! {
    struct InterruptEnable: u32 {
        /// XScale UART Unit Enable (UUE).
        const UART_UNIT_ENABLE = 1 << 6;
    }

    struct LineControl: u32 {
        const WORD_LENGTH_8 = 0b11;
        const DIVISOR_LATCH_ACCESS = 1 << 7;
    }

    struct FifoControl: u32 {
        const FIFO_ENABLE = 1 << 0;
        const RX_FIFO_RESET = 1 << 1;
        const TX_FIFO_RESET = 1 << 2;
    }

    struct LineStatus: u32 {
        const DATA_READY = 1 << 0;
        const TX_HOLDING_REGISTER_EMPTY = 1 << 5;
    }
}

/// The 16-bit divisor shared by the DLL and DLH registers.
struct BaudDivisor(u16);

impl BaudDivisor {
    fn bytes(self) -> [u8; size_of::<u16>()] {
        self.0.to_le_bytes()
    }
}

/// Whether initialization preserves or replaces the inherited baud divisor.
enum BaudSetup {
    Preserve,
    Program(BaudDivisor),
}

impl BaudSetup {
    fn from_clock_hz(clock_hz: u32) -> Option<Self> {
        uart_divisor(clock_hz).map(BaudDivisor).map(Self::Program)
    }
}

fn read_reg(registers: &MmioRegion, reg: Register) -> u32 {
    registers
        .read(reg.offset())
        .expect("BUG: XScale UART register escaped its MMIO window")
}

fn write_reg(registers: &MmioRegion, reg: Register, value: u32) {
    registers
        .write(reg.offset(), value)
        .expect("BUG: XScale UART register escaped its MMIO window")
}

fn line_status(registers: &MmioRegion) -> LineStatus {
    LineStatus::from_bits_retain(read_reg(registers, Register::LineStatus))
}

fn init(registers: &MmioRegion, clock_hz: Option<u32>) -> Result<()> {
    // A device tree may name a clock provider without supplying its rate.
    // In that case, preserve the divisor installed by the previous boot stage.
    let baud = match clock_hz {
        Some(clock_hz) => BaudSetup::from_clock_hz(clock_hz).ok_or(Error::InvalidArgs)?,
        None => BaudSetup::Preserve,
    };
    configure(registers, baud);
    Ok(())
}

fn configure(registers: &MmioRegion, baud: BaudSetup) {
    // Clear DLAB before accessing IER; firmware may inherit any prior UART state.
    write_reg(
        registers,
        Register::LineControl,
        LineControl::WORD_LENGTH_8.bits(),
    );
    write_reg(
        registers,
        Register::InterruptEnableOrDivisorHigh,
        InterruptEnable::UART_UNIT_ENABLE.bits(),
    );

    if let BaudSetup::Program(divisor) = baud {
        program_baud(registers, divisor);
    }

    write_reg(
        registers,
        Register::FifoControlOrInterruptId,
        (FifoControl::FIFO_ENABLE | FifoControl::RX_FIFO_RESET | FifoControl::TX_FIFO_RESET).bits(),
    );
    write_reg(
        registers,
        Register::InterruptEnableOrDivisorHigh,
        InterruptEnable::UART_UNIT_ENABLE.bits(),
    );
}

fn program_baud(registers: &MmioRegion, divisor: BaudDivisor) {
    let [low, high] = divisor.bytes();
    write_reg(
        registers,
        Register::LineControl,
        (LineControl::DIVISOR_LATCH_ACCESS | LineControl::WORD_LENGTH_8).bits(),
    );
    write_reg(registers, Register::DataOrDivisorLow, u32::from(low));
    write_reg(
        registers,
        Register::InterruptEnableOrDivisorHigh,
        u32::from(high),
    );
    write_reg(
        registers,
        Register::LineControl,
        LineControl::WORD_LENGTH_8.bits(),
    );
}

fn write(registers: &MmioRegion, src: &[u8]) -> Result<usize, ConsoleError> {
    let mut count = 0;
    for &byte in src {
        if !line_status(registers).contains(LineStatus::TX_HOLDING_REGISTER_EMPTY) {
            break;
        }
        write_reg(registers, Register::DataOrDivisorLow, byte as u32);
        count += 1;
    }
    Ok(count)
}

fn read(registers: &MmioRegion, dst: &mut [u8]) -> Result<usize, ConsoleError> {
    let mut count = 0;
    for byte in dst.iter_mut() {
        if !line_status(registers).contains(LineStatus::DATA_READY) {
            break;
        }
        *byte = read_reg(registers, Register::DataOrDivisorLow) as u8;
        count += 1;
    }
    Ok(count)
}

struct XScale {
    registers: MmioRegion,
}

impl ConsoleDevice for XScale {
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
    Ok(Box::new(XScale { registers }))
}

/// Driver for Intel XScale/PXA and SpacemiT K1 UARTs.
pub(super) struct XscaleUartDriver;

pub(super) static XSCALE_UART: XscaleUartDriver = XscaleUartDriver;

impl Driver<Console> for XscaleUartDriver {
    fn compatibles(&self) -> &'static [&'static str] {
        &["intel,xscale-uart", "spacemit,k1-uart"]
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
        Ok(Some(Description::new(
            "xscale_uart",
            registers,
            move |memory| bind(registers, clock_hz, memory),
        )))
    }
}
