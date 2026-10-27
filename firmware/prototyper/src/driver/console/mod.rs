//! Byte-oriented console devices.
//!
//! # References
//!
//! - Hardware manual: [Texas Instruments TL16C550D data sheet](https://www.ti.com/lit/ds/symlink/tl16c550d.pdf),
//!   Programmable Baud Generator — the common 16550 divisor calculation.

mod arm;
mod bouffalo;
mod intel;
mod sifive;
mod ti;
mod xilinx;

use alloc::boxed::Box;
use core::mem::align_of;
use runtime::{
    Error, Result,
    memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion, MmioValue},
};

use super::device;

pub(crate) use super::device::Driver;

/// The console class.
pub(crate) struct Console;

impl device::Class for Console {
    type Device = Box<dyn ConsoleDevice>;
}

pub(crate) type Description = device::Description<Console>;

/// Boot-report metadata retained after the console description is bound.
pub(crate) struct ConsoleSummary {
    pub(crate) name: &'static str,
    pub(crate) registers: DeviceRegisterRange,
}

pub(crate) static DRIVERS: device::Drivers<Console> = device::Drivers::new(&[
    &arm::PL011,
    &ti::UART16550_U8,
    &ti::UART16550_U32,
    &xilinx::AXI_UART_LITE,
    &bouffalo::BL808_UART,
    &sifive::UART,
    &intel::XSCALE_UART,
]);

/// Low-level error category for one console slice operation.
///
/// These are the backend errors shared by all three DBCN functions in SBI v3.0,
/// Section 12, Tables 50-52. Lack of progress is `Ok(0)`, not an error.
///
/// Important:
/// - `InvalidParam` is intentionally absent.
///   Physical-memory-range validation and translation belong to the SBI entry layer.
/// - `write_slice` / `read_slice` operate on one concrete contiguous slice only.
///   They are not identical to SBI `write` / `read`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsoleError {
    /// Console access is not allowed; mapped to `SBI_ERR_DENIED` (-4).
    #[allow(
        dead_code,
        reason = "current UART drivers do not restrict console access"
    )]
    Denied,
    /// Backend I/O failure; mapped to `SBI_ERR_FAILED` (-1), not `SBI_ERR_IO`.
    Failed,
}

/// A console device: the byte-oriented data path of an SBI DBCN console.
pub trait ConsoleDevice: Send {
    /// Tries to write one contiguous slice.
    fn write_slice(&mut self, src: &[u8]) -> Result<usize, ConsoleError>;

    /// Tries to read one contiguous slice.
    fn read_slice(&mut self, dst: &mut [u8]) -> Result<usize, ConsoleError>;
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
) -> Result<MmioRegion> {
    let registers = registers.subrange(0, span)?;
    if !registers.has_aligned_bounds(align_of::<T>()) {
        return Err(Error::InvalidArgs);
    }
    memory.acquire(registers)
}
