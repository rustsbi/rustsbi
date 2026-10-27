//! Bouffalo Lab BL808 UART.
//!
//! # References
//!
//! - Hardware manual: [BL808 Reference Manual v1.3](https://github.com/bouffalolab/bl_docs/blob/665e7daa84b498a767e656f458f26aca3be4fd45/BL808_RM/en/BL808_RM_en_1.3.pdf),
//!   UART chapter — register layout and FIFO count fields.

use alloc::boxed::Box;
use core::mem::size_of;
use runtime::{
    Error, PlatformView, Result,
    memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion},
};

use crate::devicetree::EnabledNode;
use crate::driver::console::{
    Console, ConsoleDevice, ConsoleError, Description, Driver, acquire_registers,
};

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

fn fifo_counts(registers: &MmioRegion) -> FifoCounts {
    FifoCounts(
        registers
            .read(Register::FifoConfig1.offset())
            .expect("BUG: BL808 FIFO configuration register escaped its MMIO window"),
    )
}

fn read_u8(registers: &MmioRegion, reg: Register) -> u8 {
    registers
        .read(reg.offset())
        .expect("BUG: BL808 UART register escaped its MMIO window")
}

fn write_u8(registers: &MmioRegion, reg: Register, value: u8) {
    registers
        .write(reg.offset(), value)
        .expect("BUG: BL808 UART register escaped its MMIO window")
}

fn write(registers: &MmioRegion, src: &[u8]) -> Result<usize, ConsoleError> {
    let mut count = 0;
    for &byte in src {
        if fifo_counts(registers).tx_available_count() == 0 {
            break;
        }
        count += 1;
        write_u8(registers, Register::TxData, byte);
    }
    Ok(count)
}

fn read(registers: &MmioRegion, dst: &mut [u8]) -> Result<usize, ConsoleError> {
    let available_count = fifo_counts(registers).rx_available_count();
    if available_count == 0 {
        return Ok(0);
    }
    let len = core::cmp::min(available_count, dst.len());
    dst.iter_mut()
        .take(len)
        .for_each(|byte| *byte = read_u8(registers, Register::RxData));
    Ok(len)
}

struct Bl808 {
    registers: MmioRegion,
}

impl ConsoleDevice for Bl808 {
    fn write_slice(&mut self, src: &[u8]) -> Result<usize, ConsoleError> {
        write(&self.registers, src)
    }

    fn read_slice(&mut self, dst: &mut [u8]) -> Result<usize, ConsoleError> {
        read(&self.registers, dst)
    }
}

fn bind(
    registers: DeviceRegisterRange,
    memory: &mut MemoryRegistry,
) -> Result<Box<dyn ConsoleDevice>> {
    let registers = acquire_registers::<u32>(registers, SPAN, memory)?;
    Ok(Box::new(Bl808 { registers }))
}

/// Driver for Bouffalo Lab BL808 UARTs.
pub(super) struct Bl808UartDriver;

pub(super) static BL808_UART: Bl808UartDriver = Bl808UartDriver;

impl Driver<Console> for Bl808UartDriver {
    fn compatibles(&self) -> &'static [&'static str] {
        &["bflb,bl808-uart"]
    }

    fn parse<'a, 'tree>(
        &self,
        platform: &'a PlatformView<'tree>,
        device: EnabledNode<'a, 'tree>,
        _compatible: &str,
    ) -> Result<Option<Description>> {
        let registers = platform
            .device_register(device.node())?
            .ok_or(Error::InvalidArgs)?;
        Ok(Some(Description::new(
            "bl808_uart",
            registers,
            move |memory| bind(registers, memory),
        )))
    }
}
