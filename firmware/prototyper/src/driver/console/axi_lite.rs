//! Xilinx AXI UART Lite.
//!
//! # References
//!
//! - Hardware manual: [AMD PG142, *AXI UART Lite v2.0 Product Guide*](https://docs.amd.com/v/u/en-US/pg142-axi-uartlite) —
//!   register layout and status/control fields.

use alloc::boxed::Box;
use bitflags::bitflags;
use core::mem::size_of;
use runtime::memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion};

use crate::driver::console::{ConsoleDevice, acquire_registers};

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

pub(super) fn bind(
    registers: DeviceRegisterRange,
    memory: &mut MemoryRegistry,
) -> runtime::Result<Box<dyn ConsoleDevice>> {
    let registers = acquire_registers::<u32>(registers, SPAN, memory)?;
    Ok(Box::new(UartAxiLite::new(registers)))
}

struct UartAxiLite {
    registers: MmioRegion,
}

impl UartAxiLite {
    fn new(registers: MmioRegion) -> Self {
        Self { registers }
    }

    fn read_reg(&self, reg: Register) -> u32 {
        self.registers
            .read(reg.offset())
            .expect("BUG: UART Lite register escaped its MMIO window")
    }

    fn write_reg(&self, reg: Register, value: u32) {
        self.registers
            .write(reg.offset(), value)
            .expect("BUG: UART Lite register escaped its MMIO window")
    }

    fn status(&self) -> Status {
        Status::from_bits_retain(self.read_reg(Register::Status))
    }
}

impl ConsoleDevice for UartAxiLite {
    fn read(&self, buf: &mut [u8]) -> usize {
        let mut count = 0;
        for byte in buf.iter_mut() {
            if !self.status().contains(Status::RX_VALID) {
                break;
            }
            *byte = self.read_reg(Register::Rx) as u8;
            count += 1;
        }
        count
    }

    fn write(&self, buf: &[u8]) -> usize {
        let mut count = 0;
        for &byte in buf {
            if self.status().contains(Status::TX_FULL) {
                break;
            }
            self.write_reg(Register::Tx, byte as u32);
            count += 1;
        }
        count
    }
}
