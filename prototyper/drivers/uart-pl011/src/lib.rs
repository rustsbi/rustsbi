//! Safe Arm PL011 UART register protocol.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::boxed::Box;
use core::ops::Range;

use machine::memory::{IoMem, IoMemError, io_fence};
use machine::{Console, ConsoleDevice, ConsoleError};

struct Register;

impl Register {
    const DATA: usize = 0x00;
    const FLAGS: usize = 0x18;
    const INTEGER_BAUD: usize = 0x24;
    const FRACTIONAL_BAUD: usize = 0x28;
    const LINE_CONTROL: usize = 0x2c;
    const CONTROL: usize = 0x30;
    const INTERRUPT_MASK: usize = 0x38;
    const INTERRUPT_CLEAR: usize = 0x44;
    const RECEIVE_EMPTY: u32 = 1 << 4;
    const TRANSMIT_FULL: u32 = 1 << 5;
}

/// Claims, initializes, and binds one PL011 register window.
pub fn install(range: Range<usize>) -> Result<Console, IoMemError> {
    let io = IoMem::acquire(range)?;
    io.validate::<u32>(Register::INTERRUPT_CLEAR)?;
    let uart = Uart { io };
    uart.initialize()?;
    Ok(Console::new(Box::new(uart)))
}

struct Uart {
    io: IoMem,
}

impl Uart {
    fn initialize(&self) -> Result<(), IoMemError> {
        io_fence();
        self.io.write_once(Register::CONTROL, 0u32)?;
        self.io.write_once(Register::INTERRUPT_CLEAR, 0x7ffu32)?;
        // Fixed 24 MHz input, 115200 baud, 8-N-1 with FIFOs enabled.
        self.io.write_once(Register::INTEGER_BAUD, 13u32)?;
        self.io.write_once(Register::FRACTIONAL_BAUD, 1u32)?;
        self.io
            .write_once(Register::LINE_CONTROL, (0b11u32 << 5) | (1u32 << 4))?;
        self.io.write_once(Register::INTERRUPT_MASK, 0u32)?;
        self.io
            .write_once(Register::CONTROL, (1u32 << 9) | (1u32 << 8) | 1)?;
        io_fence();
        Ok(())
    }

    fn read_register(&self, offset: usize) -> Result<u32, ConsoleError> {
        self.io.read_once(offset).map_err(|_| ConsoleError::Failed)
    }
}

impl ConsoleDevice for Uart {
    fn read(&mut self, destination: &mut [u8]) -> Result<usize, ConsoleError> {
        io_fence();
        let mut count = 0;
        for byte in destination {
            if self.read_register(Register::FLAGS)? & Register::RECEIVE_EMPTY != 0 {
                break;
            }
            *byte = self.read_register(Register::DATA)? as u8;
            count += 1;
        }
        io_fence();
        Ok(count)
    }

    fn write(&mut self, source: &[u8]) -> Result<usize, ConsoleError> {
        io_fence();
        let mut count = 0;
        for &byte in source {
            if self.read_register(Register::FLAGS)? & Register::TRANSMIT_FULL != 0 {
                break;
            }
            self.io
                .write_once(Register::DATA, u32::from(byte))
                .map_err(|_| ConsoleError::Failed)?;
            count += 1;
        }
        io_fence();
        Ok(count)
    }
}
