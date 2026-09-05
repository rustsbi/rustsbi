//! SiFive UART.
//!
//! # References
//!
//! - Hardware manual: [SiFive FE310-G002 Manual](https://sifive.cdn.prismic.io/sifive%2F9ecbb623-7c7f-4acc-966f-9bb10ecdb62e_fe310-g002.pdf),
//!   Chapter 18 — UART register layout and fields.

use alloc::boxed::Box;
use bitflags::bitflags;
use core::mem::size_of;
use runtime::memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion};

use crate::driver::console::{ConsoleDevice, acquire_registers};

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

const SPAN: usize = Register::InterruptEnable.offset() + size_of::<u32>();

bitflags! {
    struct Control: u32 {
        const ENABLE = 1 << 0;
    }
}

pub(super) fn bind(
    registers: DeviceRegisterRange,
    memory: &mut MemoryRegistry,
) -> runtime::Result<Box<dyn ConsoleDevice>> {
    let registers = acquire_registers::<u32>(registers, SPAN, memory)?;
    Ok(Box::new(UartSiFive::new(registers)))
}

struct FifoData(u32);

impl FifoData {
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

struct UartSiFive {
    registers: MmioRegion,
}

impl UartSiFive {
    /// Enables the transmit and receive channels in the acquired register
    /// window, with interrupts disabled.
    fn new(registers: MmioRegion) -> Self {
        let uart = Self { registers };
        uart.write_reg(Register::InterruptEnable, 0);
        let receive_control =
            Control::from_bits_retain(uart.read_reg(Register::RxControl)) | Control::ENABLE;
        uart.write_reg(Register::RxControl, receive_control.bits());
        let transmit_control =
            Control::from_bits_retain(uart.read_reg(Register::TxControl)) | Control::ENABLE;
        uart.write_reg(Register::TxControl, transmit_control.bits());
        uart
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

impl ConsoleDevice for UartSiFive {
    fn read(&self, buf: &mut [u8]) -> usize {
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
        count
    }

    fn write(&self, buf: &[u8]) -> usize {
        let mut count = 0;
        for &byte in buf {
            if self.tx_fifo_full() {
                break;
            }
            self.write_reg(Register::TxData, byte as u32);
            count += 1;
        }
        count
    }
}
