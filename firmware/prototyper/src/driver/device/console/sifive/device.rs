//! SiFive UART hardware device.

use bitflags::bitflags;
use core::mem::size_of;
use runtime::{
    Result,
    console::ConsoleDevice,
    memory::{MemoryRegistry, MmioRegion},
};

use super::description::Desc;

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

/// Minimum SiFive UART register-window size required by this driver.
pub(super) const SPAN: usize = Register::InterruptEnable.offset() + size_of::<u32>();

bitflags! {
    /// SiFive UART enable bits for the transmit and receive controls.
    struct Control: u32 {
        const ENABLE = 1 << 0;
    }
}

/// Encoded FIFO data whose top bit marks an unavailable FIFO.
struct FifoData(u32);

impl FifoData {
    /// Flag marking an empty receive FIFO or a full transmit FIFO.
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

/// A SiFive UART bound to an acquired register window.
pub(super) struct Device {
    registers: MmioRegion,
}

impl Device {
    /// Enables transmit and receive in the acquired window with interrupts disabled.
    pub(super) fn init(description: Desc, memory: &mut MemoryRegistry) -> Result<Self> {
        let Desc { registers } = description;
        let uart = Self {
            registers: memory.acquire(registers)?,
        };
        uart.write_reg(Register::InterruptEnable, 0);
        let receive_control =
            Control::from_bits_retain(uart.read_reg(Register::RxControl)) | Control::ENABLE;
        uart.write_reg(Register::RxControl, receive_control.bits());
        let transmit_control =
            Control::from_bits_retain(uart.read_reg(Register::TxControl)) | Control::ENABLE;
        uart.write_reg(Register::TxControl, transmit_control.bits());
        Ok(uart)
    }

    fn read_reg(&self, reg: Register) -> u32 {
        self.registers
            .read(reg.offset())
            .expect("BUG: SiFive UART register escaped its MMIO window")
    }

    fn write_reg(&self, reg: Register, value: u32) {
        self.registers
            .write(reg.offset(), value)
            .expect("BUG: SiFive UART register escaped its MMIO window")
    }

    /// Reads one received byte, or `None` when the receive FIFO is empty.
    fn read_byte(&self) -> Option<u8> {
        FifoData(self.read_reg(Register::RxData)).received_byte()
    }

    /// Reports whether the transmit FIFO is full.
    fn tx_fifo_full(&self) -> bool {
        FifoData(self.read_reg(Register::TxData)).is_unavailable()
    }
}

impl ConsoleDevice for Device {
    fn read_slice(&mut self, buf: &mut [u8]) -> Result<usize> {
        let mut count = 0;
        for byte in buf.iter_mut() {
            match self.read_byte() {
                Some(received_byte) => {
                    *byte = received_byte;
                    count += 1;
                }
                None => break,
            }
        }
        Ok(count)
    }

    fn write_slice(&mut self, buf: &[u8]) -> Result<usize> {
        let mut count = 0;
        for &byte in buf {
            if self.tx_fifo_full() {
                break;
            }
            self.write_reg(Register::TxData, byte as u32);
            count += 1;
        }
        Ok(count)
    }
}
