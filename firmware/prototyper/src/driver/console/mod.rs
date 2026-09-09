//! Byte-oriented console backends.
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

/// Low-level error category for one backend slice operation.
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
pub enum DbcnError {
    /// Console access is not allowed; mapped to `SBI_ERR_DENIED` (-4).
    #[allow(
        dead_code,
        reason = "current UART backends do not restrict console access"
    )]
    Denied,
    /// Backend I/O failure; mapped to `SBI_ERR_FAILED` (-1), not `SBI_ERR_IO`.
    Failed,
}

/// A backend that can serve as the byte-oriented data path of an SBI DBCN console.
///
/// Important:
/// - This trait is intentionally not named `Uart`.
/// - This trait is also intentionally not phrased in terms of SBI calls.
/// - It models one backend operation on one contiguous slice.
///
/// Relationship with SBI DBCN:
/// - SBI `write` may translate one physical memory range into 1..N slices, then
///   call `write_slice` repeatedly.
/// - SBI `read` may translate one physical memory range into 1..N slices, then
///   call `read_slice` repeatedly.
/// - SBI `write_byte` uses `write_slice(&[byte])`, retrying `Ok(0)` to preserve
///   the SBI call's blocking semantics.
pub trait DbcnBackend {
    /// Try to write bytes from one contiguous source slice.
    ///
    /// Returns:
    /// - `Ok(n)` where `0 <= n <= src.len()`, meaning exactly the first `n` bytes
    ///   of `src` are accepted by the backend;
    /// - `Err(DbcnError::Denied)` if writes are not allowed;
    /// - `Err(DbcnError::Failed)` on backend I/O failure.
    ///
    /// This is a non-blocking slice operation. It is not the SBI `write` call itself.
    fn write_slice(&mut self, src: &[u8]) -> Result<usize, DbcnError>;

    /// Try to read bytes into one contiguous destination slice.
    ///
    /// Returns:
    /// - `Ok(n)` where `0 <= n <= dst.len()`, meaning exactly `n` bytes are consumed
    ///   from the backend receive queue and written into the destination slice;
    /// - `Err(DbcnError::Denied)` if reads are not allowed;
    /// - `Err(DbcnError::Failed)` on backend I/O failure.
    ///
    /// This is a non-blocking slice operation. It is not the SBI `read` call itself.
    ///
    /// Note: this abstraction specifies the backend state transition and the
    /// number of bytes read. It does not model the concrete contents of `dst` yet.
    fn read_slice(&mut self, dst: &mut [u8]) -> Result<usize, DbcnError>;
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
) -> runtime::Result<Option<Box<dyn DbcnBackend + Send>>> {
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
    };
    Ok(Some(device))
}
