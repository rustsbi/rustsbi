//! Intel XScale/PXA UART.
//!
//! # References
//!
//! - Hardware manual: [Intel PXA27x Processor Family Developer's Manual, order 280000-001](https://support.eurotech-inc.com/developers/datasheets/PXA270_DeveloperManual.pdf),
//!   Chapter 10 — full-function UART register layout and fields.

use alloc::boxed::Box;
use bitflags::bitflags;
use core::mem::size_of;
use runtime::memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion};

use crate::driver::console::{ConsoleDevice, acquire_registers, uart_divisor};

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

pub(super) fn bind(
    registers: DeviceRegisterRange,
    clock_hz: Option<u32>,
    memory: &mut MemoryRegistry,
) -> runtime::Result<Box<dyn ConsoleDevice>> {
    let clock_hz = clock_hz.ok_or(runtime::Error::InvalidArgs)?;
    let baud = BaudSetup::from_clock_hz(clock_hz).ok_or(runtime::Error::InvalidArgs)?;
    bind_with_baud(registers, baud, memory)
}

pub(super) fn bind_spacemit_k1(
    registers: DeviceRegisterRange,
    clock_hz: Option<u32>,
    memory: &mut MemoryRegistry,
) -> runtime::Result<Box<dyn ConsoleDevice>> {
    // Some K1 device trees name a clock provider without supplying its rate.
    // In that case, preserve the divisor installed by the previous boot stage.
    let baud = match clock_hz {
        Some(clock_hz) => BaudSetup::from_clock_hz(clock_hz).ok_or(runtime::Error::InvalidArgs)?,
        None => BaudSetup::Preserve,
    };
    bind_with_baud(registers, baud, memory)
}

fn bind_with_baud(
    registers: DeviceRegisterRange,
    baud: BaudSetup,
    memory: &mut MemoryRegistry,
) -> runtime::Result<Box<dyn ConsoleDevice>> {
    let registers = acquire_registers::<u32>(registers, SPAN, memory)?;
    Ok(Box::new(UartXScale::new(registers, baud)))
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

struct UartXScale {
    registers: MmioRegion,
}

impl UartXScale {
    /// Enables 8N1 operation and applies the selected baud setup.
    fn new(registers: MmioRegion, baud: BaudSetup) -> Self {
        let uart = Self { registers };
        uart.configure(baud);
        uart
    }

    fn configure(&self, baud: BaudSetup) {
        // Clear DLAB before accessing IER; firmware may inherit any prior UART state.
        self.write_reg(Register::LineControl, LineControl::WORD_LENGTH_8.bits());
        self.write_reg(
            Register::InterruptEnableOrDivisorHigh,
            InterruptEnable::UART_UNIT_ENABLE.bits(),
        );

        if let BaudSetup::Program(divisor) = baud {
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

    fn program_baud(&self, divisor: BaudDivisor) {
        let [low, high] = divisor.bytes();
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

impl ConsoleDevice for UartXScale {
    fn read(&self, buf: &mut [u8]) -> usize {
        let mut count = 0;
        for byte in buf.iter_mut() {
            if !self.line_status().contains(LineStatus::DATA_READY) {
                break;
            }
            *byte = self.read_reg(Register::DataOrDivisorLow) as u8;
            count += 1;
        }
        count
    }

    fn write(&self, buf: &[u8]) -> usize {
        for &byte in buf {
            while !self
                .line_status()
                .contains(LineStatus::TX_HOLDING_REGISTER_EMPTY)
            {
                core::hint::spin_loop();
            }
            self.write_reg(Register::DataOrDivisorLow, byte as u32);
        }
        buf.len()
    }
}
