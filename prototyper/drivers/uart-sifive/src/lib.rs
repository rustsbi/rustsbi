//! Safe SiFive UART register protocol.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::boxed::Box;
use machine::{Console, ConsoleDevice, ConsoleError, IoMem, IoMemError, io_fence};

struct Register;

impl Register {
    const TX_DATA: usize = 0x00;
    const RX_DATA: usize = 0x04;
    const TX_CONTROL: usize = 0x08;
    const RX_CONTROL: usize = 0x0c;
    const INTERRUPT_ENABLE: usize = 0x10;
    const DIVISOR: usize = 0x18;
    const EMPTY: u32 = 1 << 31;
}

/// Initializes and binds one already-owned SiFive UART window.
pub fn bind(io: IoMem) -> Result<Console, IoMemError> {
    io.validate::<u32>(Register::DIVISOR)?;
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
        self.io.write_once(Register::INTERRUPT_ENABLE, 0u32)?;
        let receive: u32 = self.io.read_once(Register::RX_CONTROL)?;
        self.io.write_once(Register::RX_CONTROL, receive | 1)?;
        let transmit: u32 = self.io.read_once(Register::TX_CONTROL)?;
        self.io.write_once(Register::TX_CONTROL, transmit | 1)?;
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
            let value = self.read_register(Register::RX_DATA)?;
            if value & Register::EMPTY != 0 {
                break;
            }
            *byte = value as u8;
            count += 1;
        }
        io_fence();
        Ok(count)
    }

    fn write(&mut self, source: &[u8]) -> Result<usize, ConsoleError> {
        io_fence();
        let mut count = 0;
        for &byte in source {
            if self.read_register(Register::TX_DATA)? & Register::EMPTY != 0 {
                break;
            }
            self.io
                .write_once(Register::TX_DATA, u32::from(byte))
                .map_err(|_| ConsoleError::Failed)?;
            count += 1;
        }
        io_fence();
        Ok(count)
    }
}
