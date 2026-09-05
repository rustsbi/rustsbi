//! UART console drivers.
//!
//! # References
//!
//! - Hardware manual: [Texas Instruments TL16C550D data sheet](https://www.ti.com/lit/ds/symlink/tl16c550d.pdf),
//!   Programmable Baud Generator — the common 16550 divisor calculation.

mod axi_lite;
mod bl808;
mod kind;
mod pl011;
mod sifive;
mod uart16550;
mod xscale;

use alloc::boxed::Box;
use core::mem::align_of;

use runtime::memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion, MmioValue};

use crate::platform::BoardInfo;

pub(crate) use kind::ConsoleKind;

/// A byte-oriented console device.
pub(crate) trait ConsoleDevice: Send {
    /// Reads bytes into `buf` and returns the number read.
    fn read(&self, buf: &mut [u8]) -> usize;

    /// Writes bytes from `buf` and returns the number written.
    fn write(&self, buf: &[u8]) -> usize;
}

pub(super) const BAUD_RATE: u32 = 115_200;

// 16550-compatible UARTs divide their input clock by sixteen before applying
// the programmable divisor.
const UART_CLOCK_OVERSAMPLING: u32 = 16;

/// Returns the nearest 16550 integer divisor.
pub(crate) fn uart_divisor(clock_hz: u32) -> Option<u16> {
    let denominator = BAUD_RATE.checked_mul(UART_CLOCK_OVERSAMPLING)?;
    let divisor = clock_hz.checked_add(denominator / 2)? / denominator;
    (1..=u16::MAX as u32)
        .contains(&divisor)
        .then_some(divisor as u16)
}

fn acquire_registers<T: MmioValue>(
    registers: DeviceRegisterRange,
    span: usize,
    memory: &mut MemoryRegistry,
) -> runtime::Result<MmioRegion> {
    let registers = registers.subrange(0, span)?;
    if !registers.has_aligned_bounds(align_of::<T>()) {
        return Err(runtime::Error::InvalidArgs);
    }
    memory.acquire_mmio(registers)
}

/// Binds the console selected during platform discovery.
pub(super) fn bind(
    board: &BoardInfo,
    memory: &mut MemoryRegistry,
) -> runtime::Result<Option<Box<dyn ConsoleDevice>>> {
    let Some(console) = board.console.as_ref() else {
        return Ok(None);
    };
    let device = match console.kind {
        ConsoleKind::Uart16550U8 => uart16550::bind_u8(console.registers, memory)?,
        ConsoleKind::Uart16550U32 => uart16550::bind_u32(console.registers, memory)?,
        ConsoleKind::AxiLite => axi_lite::bind(console.registers, memory)?,
        ConsoleKind::Bl808 => bl808::bind(console.registers, memory)?,
        ConsoleKind::SiFive => sifive::bind(console.registers, memory)?,
        ConsoleKind::Pl011 => pl011::bind(console.registers, console.clock_hz, memory)?,
        ConsoleKind::XScale => xscale::bind(console.registers, console.clock_hz, memory)?,
        ConsoleKind::SpacemitK1 => {
            xscale::bind_spacemit_k1(console.registers, console.clock_hz, memory)?
        }
    };
    Ok(Some(device))
}
