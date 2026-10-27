//! Xilinx AXI UART Lite.
//!
//! # References
//!
//! - Hardware manual: [AMD PG142, *AXI UART Lite v2.0 Product Guide*](https://docs.amd.com/v/u/en-US/pg142-axi-uartlite) —
//!   register layout and status/control fields.

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

/// Register offsets within the UART Lite register map.
#[repr(usize)]
#[derive(Clone, Copy)]
enum Register {
    /// Receive FIFO data.
    Rx = 0x00,
    /// Transmit FIFO data.
    Tx = 0x04,
    /// Status flags.
    Status = 0x08,
}

impl Register {
    /// Byte offset of this register.
    const fn offset(self) -> usize {
        self as usize
    }
}

const SPAN: usize = Register::Status.offset() + size_of::<u32>();

bitflags! {
    struct Status: u32 {
        const RX_VALID = 1 << 0;
        const TX_FULL = 1 << 3;
    }
}

fn read_reg(registers: &MmioRegion, reg: Register) -> u32 {
    registers
        .read(reg.offset())
        .expect("BUG: UART Lite register escaped its MMIO window")
}

fn write_reg(registers: &MmioRegion, reg: Register, value: u32) {
    registers
        .write(reg.offset(), value)
        .expect("BUG: UART Lite register escaped its MMIO window")
}

fn status(registers: &MmioRegion) -> Status {
    Status::from_bits_retain(read_reg(registers, Register::Status))
}

fn write(registers: &MmioRegion, src: &[u8]) -> Result<usize, ConsoleError> {
    let mut count = 0;
    for &byte in src {
        if status(registers).contains(Status::TX_FULL) {
            break;
        }
        write_reg(registers, Register::Tx, byte as u32);
        count += 1;
    }
    Ok(count)
}

fn read(registers: &MmioRegion, dst: &mut [u8]) -> Result<usize, ConsoleError> {
    let mut count = 0;
    for byte in dst.iter_mut() {
        if !status(registers).contains(Status::RX_VALID) {
            break;
        }
        *byte = read_reg(registers, Register::Rx) as u8;
        count += 1;
    }
    Ok(count)
}

struct AxiLite {
    registers: MmioRegion,
}

impl ConsoleDevice for AxiLite {
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
    Ok(Box::new(AxiLite { registers }))
}

/// Driver for Xilinx AXI UART Lite.
pub(super) struct AxiUartLiteDriver;

pub(super) static AXI_UART_LITE: AxiUartLiteDriver = AxiUartLiteDriver;

impl Driver<Console> for AxiUartLiteDriver {
    fn compatibles(&self) -> &'static [&'static str] {
        &["xlnx,xps-uartlite-1.00.a"]
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
            "axi_lite_uart",
            registers,
            move |memory| bind(registers, memory),
        )))
    }
}
