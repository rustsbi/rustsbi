//! Safe NS16550 and DW-APB UART register protocol.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::boxed::Box;
use machine::{Console, ConsoleDevice, ConsoleError, IoMem, IoMemError, io_fence};

/// Register access convention selected by the compatible binding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Access {
    /// NS16550 byte registers at consecutive offsets.
    Byte,
    /// DW-APB 32-bit registers with a four-byte stride.
    Word,
}

/// Binds an already-owned 16550-family register window.
pub fn bind(io: IoMem, access: Access) -> Result<Console, IoMemError> {
    match access {
        Access::Byte => io.validate::<u8>(LineStatus::OFFSET)?,
        Access::Word => io.validate::<u32>(LineStatus::OFFSET * 4)?,
    }
    Ok(Console::new(Box::new(Uart { io, access })))
}

struct Receive;
struct Transmit;
struct LineStatus;

impl Receive {
    const OFFSET: usize = 0;
}

impl Transmit {
    const OFFSET: usize = 0;
}

impl LineStatus {
    /// LSR bit 0 is data-ready; bit 5 is transmitter-holding-register empty.
    const OFFSET: usize = 5;
    const DATA_READY: u8 = 1;
    const TRANSMIT_READY: u8 = 1 << 5;
}

struct Uart {
    io: IoMem,
    access: Access,
}

impl Uart {
    fn read_register(&self, index: usize) -> Result<u8, ConsoleError> {
        match self.access {
            Access::Byte => self.io.read_once(index),
            Access::Word => self.io.read_once::<u32>(index * 4).map(|value| value as u8),
        }
        .map_err(|_| ConsoleError::Failed)
    }

    fn write_register(&self, index: usize, value: u8) -> Result<(), ConsoleError> {
        match self.access {
            Access::Byte => self.io.write_once(index, value),
            Access::Word => self.io.write_once(index * 4, u32::from(value)),
        }
        .map_err(|_| ConsoleError::Failed)
    }
}

impl ConsoleDevice for Uart {
    fn read(&mut self, destination: &mut [u8]) -> Result<usize, ConsoleError> {
        io_fence();
        let mut count = 0;
        for byte in destination {
            if self.read_register(LineStatus::OFFSET)? & LineStatus::DATA_READY == 0 {
                break;
            }
            *byte = self.read_register(Receive::OFFSET)?;
            count += 1;
        }
        io_fence();
        Ok(count)
    }

    fn write(&mut self, source: &[u8]) -> Result<usize, ConsoleError> {
        io_fence();
        let mut count = 0;
        for &byte in source {
            if self.read_register(LineStatus::OFFSET)? & LineStatus::TRANSMIT_READY == 0 {
                break;
            }
            self.write_register(Transmit::OFFSET, byte)?;
            count += 1;
        }
        io_fence();
        Ok(count)
    }
}
