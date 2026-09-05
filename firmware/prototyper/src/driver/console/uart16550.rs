//! NS16550-compatible UART with byte- or word-spaced registers.
//!
//! # References
//!
//! - Hardware manual: [Texas Instruments TL16C550D data sheet](https://www.ti.com/lit/ds/symlink/tl16c550d.pdf),
//!   Register Functional Description — register indices and line-status fields.

use alloc::boxed::Box;
use bitflags::bitflags;
use core::mem::size_of;
use runtime::memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion};

use crate::driver::console::{ConsoleDevice, acquire_registers};

#[repr(usize)]
#[derive(Clone, Copy)]
enum Register {
    Data = 0,
    LineStatus = 5,
}

impl Register {
    const fn index(self) -> usize {
        self as usize
    }
}

const U8_SPAN: usize = Register::LineStatus.index() + size_of::<u8>();
const U32_SPAN: usize = (Register::LineStatus.index() + 1) * size_of::<u32>();

bitflags! {
    struct LineStatus: u8 {
        const DATA_READY = 1 << 0;
        const TX_HOLDING_REGISTER_EMPTY = 1 << 5;
    }
}

pub(super) fn bind_u8(
    registers: DeviceRegisterRange,
    memory: &mut MemoryRegistry,
) -> runtime::Result<Box<dyn ConsoleDevice>> {
    let registers = acquire_registers::<u8>(registers, U8_SPAN, memory)?;
    Ok(Box::new(Uart16550::new(U8RegisterAccess(registers))))
}

pub(super) fn bind_u32(
    registers: DeviceRegisterRange,
    memory: &mut MemoryRegistry,
) -> runtime::Result<Box<dyn ConsoleDevice>> {
    let registers = acquire_registers::<u32>(registers, U32_SPAN, memory)?;
    Ok(Box::new(Uart16550::new(U32RegisterAccess(registers))))
}

trait RegisterAccess {
    fn read(&self, register: Register) -> u8;
    fn write(&self, register: Register, value: u8);
}

struct U8RegisterAccess(MmioRegion);

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

struct U32RegisterAccess(MmioRegion);

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

struct Uart16550<Access> {
    registers: Access,
}

impl<Access: RegisterAccess> Uart16550<Access> {
    fn new(registers: Access) -> Self {
        Self { registers }
    }

    fn line_status(&self) -> LineStatus {
        LineStatus::from_bits_retain(self.registers.read(Register::LineStatus))
    }
}

impl<Access: RegisterAccess + Send> ConsoleDevice for Uart16550<Access> {
    fn read(&self, buf: &mut [u8]) -> usize {
        let mut count = 0;
        for byte in buf.iter_mut() {
            if !self.line_status().contains(LineStatus::DATA_READY) {
                break;
            }
            *byte = self.registers.read(Register::Data);
            count += 1;
        }
        count
    }

    fn write(&self, buf: &[u8]) -> usize {
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
        count
    }
}
