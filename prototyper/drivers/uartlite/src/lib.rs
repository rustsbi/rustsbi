//! Safe Xilinx UARTLite register protocol.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::boxed::Box;
use machine::{Console, ConsoleDevice, ConsoleError, IoMem, IoMemError, io_fence};

struct Register;

impl Register {
    const RX_FIFO: usize = 0x00;
    const TX_FIFO: usize = 0x04;
    const STATUS: usize = 0x08;
    const RX_VALID: u32 = 1;
    const TX_FULL: u32 = 1 << 3;
}

/// Binds one already-owned UARTLite register window.
pub fn bind(io: IoMem) -> Result<Console, IoMemError> {
    io.validate::<u32>(Register::STATUS)?;
    Ok(Console::new(Box::new(Uart { io })))
}

struct Uart {
    io: IoMem,
}

impl Uart {
    fn read_register(&self, offset: usize) -> Result<u32, ConsoleError> {
        self.io.read_once(offset).map_err(|_| ConsoleError::Failed)
    }
}

impl ConsoleDevice for Uart {
    fn read(&mut self, destination: &mut [u8]) -> Result<usize, ConsoleError> {
        io_fence();
        let mut count = 0;
        for byte in destination {
            if self.read_register(Register::STATUS)? & Register::RX_VALID == 0 {
                break;
            }
            *byte = self.read_register(Register::RX_FIFO)? as u8;
            count += 1;
        }
        io_fence();
        Ok(count)
    }

    fn write(&mut self, source: &[u8]) -> Result<usize, ConsoleError> {
        io_fence();
        let mut count = 0;
        for &byte in source {
            if self.read_register(Register::STATUS)? & Register::TX_FULL != 0 {
                break;
            }
            self.io
                .write_once(Register::TX_FIFO, u32::from(byte))
                .map_err(|_| ConsoleError::Failed)?;
            count += 1;
        }
        io_fence();
        Ok(count)
    }
}
