//! SiFive UART.
//!
//! # References
//!
//! - Hardware manual: [SiFive FE310-G002 Manual](https://sifive.cdn.prismic.io/sifive%2F9ecbb623-7c7f-4acc-966f-9bb10ecdb62e_fe310-g002.pdf),
//!   Chapter 18 — UART register layout and fields.

use alloc::boxed::Box;
use bitflags::bitflags;
use core::mem::size_of;
use runtime::{
    Error, PlatformView, Result,
    memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion},
};

use crate::devicetree::EnabledNode;
use crate::driver::console::{
    Console, ConsoleDevice, ConsoleError, Description, Driver, acquire_registers,
};

/// Register offsets within the SiFive UART register map.
#[repr(usize)]
#[derive(Clone, Copy)]
enum Register {
    /// Transmit data.
    TxData = 0x00,
    /// Receive data.
    RxData = 0x04,
    /// Transmit control (watermark and enable).
    TxControl = 0x08,
    /// Receive control (watermark and enable).
    RxControl = 0x0c,
    /// Interrupt enable.
    InterruptEnable = 0x10,
}

impl Register {
    /// Byte offset of this register.
    const fn offset(self) -> usize {
        self as usize
    }
}

const SPAN: usize = Register::InterruptEnable.offset() + size_of::<u32>();

bitflags! {
    struct Control: u32 {
        const ENABLE = 1 << 0;
    }
}

struct FifoData(u32);

impl FifoData {
    const UNAVAILABLE: u32 = 1 << 31;

    fn received_byte(self) -> Option<u8> {
        if self.is_unavailable() {
            None
        } else {
            Some(self.0 as u8)
        }
    }

    fn is_unavailable(&self) -> bool {
        self.0 & Self::UNAVAILABLE != 0
    }
}

fn read_reg(registers: &MmioRegion, reg: Register) -> u32 {
    registers
        .read(reg.offset())
        .expect("BUG: SiFive UART register escaped its MMIO window")
}

fn write_reg(registers: &MmioRegion, reg: Register, value: u32) {
    registers
        .write(reg.offset(), value)
        .expect("BUG: SiFive UART register escaped its MMIO window")
}

fn read_byte(registers: &MmioRegion) -> Option<u8> {
    FifoData(read_reg(registers, Register::RxData)).received_byte()
}

fn tx_fifo_full(registers: &MmioRegion) -> bool {
    FifoData(read_reg(registers, Register::TxData)).is_unavailable()
}

fn init(registers: &MmioRegion) {
    write_reg(registers, Register::InterruptEnable, 0);
    let receive_control =
        Control::from_bits_retain(read_reg(registers, Register::RxControl)) | Control::ENABLE;
    write_reg(registers, Register::RxControl, receive_control.bits());
    let transmit_control =
        Control::from_bits_retain(read_reg(registers, Register::TxControl)) | Control::ENABLE;
    write_reg(registers, Register::TxControl, transmit_control.bits());
}

fn write(registers: &MmioRegion, src: &[u8]) -> Result<usize, ConsoleError> {
    let mut count = 0;
    for &byte in src {
        if tx_fifo_full(registers) {
            break;
        }
        write_reg(registers, Register::TxData, byte as u32);
        count += 1;
    }
    Ok(count)
}

fn read(registers: &MmioRegion, dst: &mut [u8]) -> Result<usize, ConsoleError> {
    let mut count = 0;
    for byte in dst.iter_mut() {
        match read_byte(registers) {
            Some(received_byte) => {
                *byte = received_byte;
                count += 1;
            }
            None => break,
        }
    }
    Ok(count)
}

struct SiFive {
    registers: MmioRegion,
}

impl ConsoleDevice for SiFive {
    fn write_slice(&mut self, src: &[u8]) -> Result<usize, ConsoleError> {
        write(&self.registers, src)
    }

    fn read_slice(&mut self, dst: &mut [u8]) -> Result<usize, ConsoleError> {
        read(&self.registers, dst)
    }
}

fn bind(
    registers: DeviceRegisterRange,
    memory: &mut MemoryRegistry,
) -> Result<Box<dyn ConsoleDevice>> {
    let registers = acquire_registers::<u32>(registers, SPAN, memory)?;
    init(&registers);
    Ok(Box::new(SiFive { registers }))
}

/// Driver for SiFive UARTs.
pub(super) struct SifiveUartDriver;

pub(super) static UART: SifiveUartDriver = SifiveUartDriver;

impl Driver<Console> for SifiveUartDriver {
    fn compatibles(&self) -> &'static [&'static str] {
        &["sifive,uart0"]
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
        Ok(Some(Description::new(
            "sifive_uart",
            registers,
            move |memory| bind(registers, memory),
        )))
    }
}
