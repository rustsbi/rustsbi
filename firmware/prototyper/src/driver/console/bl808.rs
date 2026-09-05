//! Bouffalo Lab BL808 UART.
//!
//! # References
//!
//! - Hardware manual: [BL808 Reference Manual v1.3](https://github.com/bouffalolab/bl_docs/blob/665e7daa84b498a767e656f458f26aca3be4fd45/BL808_RM/en/BL808_RM_en_1.3.pdf),
//!   UART chapter — register layout and FIFO count fields.

use alloc::boxed::Box;
use core::mem::size_of;
use runtime::memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion};

use crate::driver::console::{ConsoleDevice, acquire_registers};

#[repr(usize)]
#[derive(Clone, Copy)]
enum Register {
    FifoConfig1 = 0x84,
    TxData = 0x88,
    RxData = 0x8c,
}

impl Register {
    const fn offset(self) -> usize {
        self as usize
    }
}

const SPAN: usize = Register::RxData.offset() + size_of::<u32>();

pub(super) fn bind(
    registers: DeviceRegisterRange,
    memory: &mut MemoryRegistry,
) -> runtime::Result<Box<dyn ConsoleDevice>> {
    let registers = acquire_registers::<u32>(registers, SPAN, memory)?;
    Ok(Box::new(UartBl808::new(registers)))
}

struct FifoCounts(u32);

impl FifoCounts {
    const COUNT_MASK: u32 = 0x3f;
    const RX_COUNT_SHIFT: u32 = 8;

    fn tx_available_count(&self) -> usize {
        (self.0 & Self::COUNT_MASK) as usize
    }

    fn rx_available_count(&self) -> usize {
        ((self.0 >> Self::RX_COUNT_SHIFT) & Self::COUNT_MASK) as usize
    }
}

struct UartBl808 {
    registers: MmioRegion,
}

impl UartBl808 {
    fn new(registers: MmioRegion) -> Self {
        Self { registers }
    }

    fn fifo_counts(&self) -> FifoCounts {
        FifoCounts(
            self.registers
                .read(Register::FifoConfig1.offset())
                .expect("BUG: BL808 FIFO configuration register escaped its MMIO window"),
        )
    }

    fn read_u8(&self, reg: Register) -> u8 {
        self.registers
            .read(reg.offset())
            .expect("BUG: BL808 UART register escaped its MMIO window")
    }

    fn write_u8(&self, reg: Register, value: u8) {
        self.registers
            .write(reg.offset(), value)
            .expect("BUG: BL808 UART register escaped its MMIO window")
    }
}

impl ConsoleDevice for UartBl808 {
    fn read(&self, buf: &mut [u8]) -> usize {
        let available_count = self.fifo_counts().rx_available_count();
        if available_count == 0 {
            return 0;
        }
        let len = core::cmp::min(available_count, buf.len());
        buf.iter_mut()
            .take(len)
            .for_each(|byte| *byte = self.read_u8(Register::RxData));
        len
    }

    fn write(&self, buf: &[u8]) -> usize {
        let mut count = 0;
        for &byte in buf {
            if self.fifo_counts().tx_available_count() == 0 {
                break;
            }
            count += 1;
            self.write_u8(Register::TxData, byte);
        }
        count
    }
}
