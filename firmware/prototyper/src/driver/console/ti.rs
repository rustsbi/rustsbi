//! NS16550-compatible UART with byte- or word-spaced registers.
//!
//! # References
//!
//! - Hardware manual: [Texas Instruments TL16C550D data sheet](https://www.ti.com/lit/ds/symlink/tl16c550d.pdf),
//!   Register Functional Description — register indices and line-status fields.

use alloc::boxed::Box;
use bitflags::bitflags;
use core::mem::size_of;
use runtime::{
    Error, PlatformView, Result,
    memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion},
};

use crate::devicetree::{EnabledNode, u32_property};
use crate::driver::console::{
    Console, ConsoleDevice, ConsoleError, Description, Driver, acquire_registers,
};

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

trait RegisterAccess {
    fn read(registers: &MmioRegion, register: Register) -> u8;
    fn write(registers: &MmioRegion, register: Register, value: u8);
}

struct U8RegisterAccess;

impl RegisterAccess for U8RegisterAccess {
    fn read(registers: &MmioRegion, register: Register) -> u8 {
        registers
            .read(register.index())
            .expect("BUG: 16550 u8 register escaped its MMIO window")
    }

    fn write(registers: &MmioRegion, register: Register, value: u8) {
        registers
            .write(register.index(), value)
            .expect("BUG: 16550 u8 register escaped its MMIO window")
    }
}

struct U32RegisterAccess;

impl RegisterAccess for U32RegisterAccess {
    fn read(registers: &MmioRegion, register: Register) -> u8 {
        registers
            .read::<u32>(register.index() * size_of::<u32>())
            .expect("BUG: 16550 u32 register escaped its MMIO window") as u8
    }

    fn write(registers: &MmioRegion, register: Register, value: u8) {
        registers
            .write(register.index() * size_of::<u32>(), u32::from(value))
            .expect("BUG: 16550 u32 register escaped its MMIO window")
    }
}

fn line_status<Access: RegisterAccess>(registers: &MmioRegion) -> LineStatus {
    LineStatus::from_bits_retain(Access::read(registers, Register::LineStatus))
}

fn read_slice<Access: RegisterAccess>(
    registers: &MmioRegion,
    dst: &mut [u8],
) -> Result<usize, ConsoleError> {
    let mut count = 0;
    for byte in dst.iter_mut() {
        if !line_status::<Access>(registers).contains(LineStatus::DATA_READY) {
            break;
        }
        *byte = Access::read(registers, Register::Data);
        count += 1;
    }
    Ok(count)
}

fn write_slice<Access: RegisterAccess>(
    registers: &MmioRegion,
    src: &[u8],
) -> Result<usize, ConsoleError> {
    let mut count = 0;
    for &byte in src {
        if !line_status::<Access>(registers).contains(LineStatus::TX_HOLDING_REGISTER_EMPTY) {
            break;
        }
        Access::write(registers, Register::Data, byte);
        count += 1;
    }
    Ok(count)
}

struct Uart16550U8 {
    registers: MmioRegion,
}

impl ConsoleDevice for Uart16550U8 {
    fn write_slice(&mut self, src: &[u8]) -> Result<usize, ConsoleError> {
        write_slice::<U8RegisterAccess>(&self.registers, src)
    }

    fn read_slice(&mut self, dst: &mut [u8]) -> Result<usize, ConsoleError> {
        read_slice::<U8RegisterAccess>(&self.registers, dst)
    }
}

struct Uart16550U32 {
    registers: MmioRegion,
}

impl ConsoleDevice for Uart16550U32 {
    fn write_slice(&mut self, src: &[u8]) -> Result<usize, ConsoleError> {
        write_slice::<U32RegisterAccess>(&self.registers, src)
    }

    fn read_slice(&mut self, dst: &mut [u8]) -> Result<usize, ConsoleError> {
        read_slice::<U32RegisterAccess>(&self.registers, dst)
    }
}

fn bind_u8(
    registers: DeviceRegisterRange,
    memory: &mut MemoryRegistry,
) -> Result<Box<dyn ConsoleDevice>> {
    let registers = acquire_registers::<u8>(registers, U8_SPAN, memory)?;
    Ok(Box::new(Uart16550U8 { registers }))
}

fn bind_u32(
    registers: DeviceRegisterRange,
    memory: &mut MemoryRegistry,
) -> Result<Box<dyn ConsoleDevice>> {
    let registers = acquire_registers::<u32>(registers, U32_SPAN, memory)?;
    Ok(Box::new(Uart16550U32 { registers }))
}

/// Driver for byte-spaced NS16550-compatible UARTs.
pub(super) struct Uart16550U8Driver;

pub(super) static UART16550_U8: Uart16550U8Driver = Uart16550U8Driver;

impl Driver<Console> for Uart16550U8Driver {
    fn compatibles(&self) -> &'static [&'static str] {
        &["ns16550", "ns16550a"]
    }

    fn parse<'a, 'tree>(
        &self,
        platform: &'a PlatformView<'tree>,
        device: EnabledNode<'a, 'tree>,
        _compatible: &str,
    ) -> Result<Option<Description>> {
        let shift = u32_property(device.node(), "reg-shift").unwrap_or(0);
        let width = u32_property(device.node(), "reg-io-width").unwrap_or(1);
        if shift != 0 || width != 1 {
            return Ok(None);
        }
        let registers = platform
            .device_register(device.node())?
            .ok_or(Error::InvalidArgs)?;
        Ok(Some(Description::new(
            "uart16550_u8",
            registers,
            move |memory| bind_u8(registers, memory),
        )))
    }
}

/// Driver for word-spaced NS16550-compatible UARTs.
pub(super) struct Uart16550U32Driver;

pub(super) static UART16550_U32: Uart16550U32Driver = Uart16550U32Driver;

impl Driver<Console> for Uart16550U32Driver {
    fn compatibles(&self) -> &'static [&'static str] {
        &[
            "ns16550",
            "ns16550a",
            "snps,dw-apb-uart",
            "allwinner,sunxi-uart",
        ]
    }

    fn parse<'a, 'tree>(
        &self,
        platform: &'a PlatformView<'tree>,
        device: EnabledNode<'a, 'tree>,
        compatible: &str,
    ) -> Result<Option<Description>> {
        // DesignWare and Allwinner compatibles select the 32-bit register
        // layout by identity, matching the previous ConsoleKind behavior.
        // A plain 16550 needs an explicit 32-bit layout.
        if compatible == "ns16550" || compatible == "ns16550a" {
            let shift = u32_property(device.node(), "reg-shift");
            let width = u32_property(device.node(), "reg-io-width");
            if shift != Some(2) || width != Some(4) {
                return Ok(None);
            }
        }
        let registers = platform
            .device_register(device.node())?
            .ok_or(Error::InvalidArgs)?;
        Ok(Some(Description::new(
            "uart16550_u32",
            registers,
            move |memory| bind_u32(registers, memory),
        )))
    }
}
