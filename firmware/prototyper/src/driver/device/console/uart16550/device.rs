//! NS16550-compatible UART hardware device.

use bitflags::bitflags;
use core::mem::size_of;
use runtime::{
    Result,
    console::ConsoleDevice,
    memory::{MemoryRegistry, MmioRegion},
};

use super::description::{DescU8, DescU32};

/// NS16550 register indices.
#[repr(usize)]
#[derive(Clone, Copy)]
pub(super) enum Register {
    Data = 0,
    LineStatus = 5,
}

impl Register {
    const fn index(self) -> usize {
        self as usize
    }
}

/// Minimum byte-spaced register-window size required by this driver.
pub(super) const U8_SPAN: usize = Register::LineStatus.index() + size_of::<u8>();
/// Minimum word-spaced register-window size required by this driver.
pub(super) const U32_SPAN: usize = (Register::LineStatus.index() + 1) * size_of::<u32>();

bitflags! {
    /// 16550 line-status bits for received data and transmitter readiness.
    struct LineStatus: u8 {
        const DATA_READY = 1 << 0;
        const TX_HOLDING_REGISTER_EMPTY = 1 << 5;
    }
}

/// Byte-wise access to a 16550 register map.
pub(super) trait RegisterAccess: Send {
    fn read(&self, register: Register) -> u8;
    fn write(&self, register: Register, value: u8);
}

/// Byte-spaced 16550 register access.
pub(super) struct U8RegisterAccess(MmioRegion);

impl RegisterAccess for U8RegisterAccess {
    fn read(&self, register: Register) -> u8 {
        self.0
            .read(register.index())
            .expect("BUG: 16550 u8 register escaped its MMIO window")
    }

    fn write(&self, register: Register, value: u8) {
        self.0
            .write(register.index(), value)
            .expect("BUG: 16550 u8 register escaped its MMIO window")
    }
}

/// Word-spaced 16550 register access using the low byte of each word.
pub(super) struct U32RegisterAccess(MmioRegion);

impl RegisterAccess for U32RegisterAccess {
    fn read(&self, register: Register) -> u8 {
        self.0
            .read::<u32>(register.index() * size_of::<u32>())
            .expect("BUG: 16550 u32 register escaped its MMIO window") as u8
    }

    fn write(&self, register: Register, value: u8) {
        self.0
            .write(register.index() * size_of::<u32>(), u32::from(value))
            .expect("BUG: 16550 u32 register escaped its MMIO window")
    }
}

/// A 16550 UART that uses `Access` for register I/O.
pub(super) struct Device<Access> {
    registers: Access,
}

impl Device<U8RegisterAccess> {
    /// Acquires a byte-spaced register window.
    pub(super) fn init(description: DescU8, memory: &mut MemoryRegistry) -> Result<Self> {
        let DescU8 { registers } = description;
        Ok(Self {
            registers: U8RegisterAccess(memory.acquire(registers)?),
        })
    }
}

impl Device<U32RegisterAccess> {
    /// Acquires a word-spaced register window.
    pub(super) fn init(description: DescU32, memory: &mut MemoryRegistry) -> Result<Self> {
        let DescU32 { registers } = description;
        Ok(Self {
            registers: U32RegisterAccess(memory.acquire(registers)?),
        })
    }
}

impl<Access: RegisterAccess> Device<Access> {
    fn line_status(&self) -> LineStatus {
        LineStatus::from_bits_retain(self.registers.read(Register::LineStatus))
    }
}

impl<Access: RegisterAccess> ConsoleDevice for Device<Access> {
    fn read_slice(&mut self, buf: &mut [u8]) -> Result<usize> {
        let mut count = 0;
        for byte in buf.iter_mut() {
            if !self.line_status().contains(LineStatus::DATA_READY) {
                break;
            }
            *byte = self.registers.read(Register::Data);
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
            self.registers.write(Register::Data, byte);
            count += 1;
        }
        Ok(count)
    }
}
