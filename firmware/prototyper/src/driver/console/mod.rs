//! Byte-oriented console devices and synchronized access.
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
use core::{fmt, mem::align_of};

use runtime::memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion, MmioValue};
use spin::Mutex;

pub(crate) use kind::ConsoleKind;

/// Failure of a console device operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConsoleError {
    /// The device operation failed.
    Failed,
}

/// Non-blocking byte operations supported by a console device.
pub(crate) trait ConsoleDevice: Send {
    /// Accepts a prefix of `src`, returning its length or zero when busy.
    fn try_write(&mut self, src: &[u8]) -> Result<usize, ConsoleError>;

    /// Receives a prefix of `dst`, returning its length or zero when empty.
    fn try_read(&mut self, dst: &mut [u8]) -> Result<usize, ConsoleError>;
}

/// A console device with serialized access from all harts.
pub(crate) struct Console {
    device: Mutex<Box<dyn ConsoleDevice>>,
}

impl Console {
    /// Binds one console device to its discovered register range and clock.
    pub(crate) fn bind(
        registers: DeviceRegisterRange,
        kind: ConsoleKind,
        clock_hz: Option<u32>,
        memory: &mut MemoryRegistry,
    ) -> runtime::Result<Self> {
        let device = match kind {
            ConsoleKind::Uart16550U8 => uart16550::bind_u8(registers, memory)?,
            ConsoleKind::Uart16550U32 => uart16550::bind_u32(registers, memory)?,
            ConsoleKind::AxiLite => axi_lite::bind(registers, memory)?,
            ConsoleKind::Bl808 => bl808::bind(registers, memory)?,
            ConsoleKind::SiFive => sifive::bind(registers, memory)?,
            ConsoleKind::Pl011 => pl011::bind(registers, clock_hz, memory)?,
            ConsoleKind::XScale => xscale::bind(registers, clock_hz, memory)?,
        };
        Ok(Self {
            device: Mutex::new(device),
        })
    }

    /// Writes the prefix the device can accept without waiting for space.
    pub(crate) fn try_write(&self, src: &[u8]) -> Result<usize, ConsoleError> {
        let count = self.device.lock().try_write(src)?;
        if count > src.len() {
            return Err(ConsoleError::Failed);
        }
        Ok(count)
    }

    /// Reads the prefix currently available without waiting for input.
    pub(crate) fn try_read(&self, dst: &mut [u8]) -> Result<usize, ConsoleError> {
        let count = self.device.lock().try_read(dst)?;
        if count > dst.len() {
            return Err(ConsoleError::Failed);
        }
        Ok(count)
    }

    /// Waits until the device accepts one byte, releasing the lock between tries.
    pub(crate) fn write_byte_blocking(&self, byte: u8) -> Result<(), ConsoleError> {
        while self.try_write(&[byte])? == 0 {
            core::hint::spin_loop();
        }
        Ok(())
    }
}

impl fmt::Write for &Console {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let mut remaining = s.as_bytes();
        while !remaining.is_empty() {
            let count = self.try_write(remaining).map_err(|_| fmt::Error)?;
            if count == 0 {
                core::hint::spin_loop();
            } else {
                remaining = &remaining[count..];
            }
        }
        Ok(())
    }
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
