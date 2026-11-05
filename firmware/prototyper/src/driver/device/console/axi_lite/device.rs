//! Xilinx AXI UART Lite hardware device.

use bitflags::bitflags;
use core::mem::size_of;
use runtime::{
    Result,
    console::ConsoleDevice,
    memory::{MemoryRegistry, MmioRegion},
};

use super::description::Desc;

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

/// Minimum UART Lite register-window size required by this driver.
pub(super) const SPAN: usize = Register::Status.offset() + size_of::<u32>();

bitflags! {
    /// Status bits for the UART Lite receive and transmit FIFOs.
    struct Status: u32 {
        const RX_VALID = 1 << 0;
        const TX_FULL = 1 << 3;
    }
}

/// An AXI UART Lite bound to an acquired register window.
pub(super) struct Device {
    registers: MmioRegion,
}

impl Device {
    /// Acquires the UART Lite register window.
    pub(super) fn init(description: Desc, memory: &mut MemoryRegistry) -> Result<Self> {
        let Desc { registers } = description;
        Ok(Self {
            registers: memory.acquire(registers)?,
        })
    }

    fn read_reg(&self, reg: Register) -> Result<u32> {
        self.registers.read(reg.offset())
    }

    fn write_reg(&self, reg: Register, value: u32) -> Result<()> {
        self.registers.write(reg.offset(), value)
    }

    fn status(&self) -> Result<Status> {
        Ok(Status::from_bits_retain(self.read_reg(Register::Status)?))
    }
}

impl ConsoleDevice for Device {
    fn read_slice(&mut self, buf: &mut [u8]) -> Result<usize> {
        let mut count = 0;
        for byte in buf.iter_mut() {
            if !self.status()?.contains(Status::RX_VALID) {
                break;
            }
            *byte = self.read_reg(Register::Rx)? as u8;
            count += 1;
        }
        Ok(count)
    }

    fn write_slice(&mut self, buf: &[u8]) -> Result<usize> {
        let mut count = 0;
        for &byte in buf {
            if self.status()?.contains(Status::TX_FULL) {
                break;
            }
            self.write_reg(Register::Tx, byte as u32)?;
            count += 1;
        }
        Ok(count)
    }
}
