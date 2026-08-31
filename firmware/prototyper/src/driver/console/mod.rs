//! UART console drivers.

mod axi_lite;
mod bflb;
mod kind;
mod pl011;
mod sifive;
mod uart16550;
mod xscale;

use alloc::boxed::Box;

use crate::platform::BoardInfo;
use crate::platform::mmio::Mmio;

pub(crate) use kind::ConsoleKind;

/// A byte-oriented console device.
pub(crate) trait ConsoleDevice: Send {
    /// Reads bytes into `buf` and returns the number read.
    fn read(&self, buf: &mut [u8]) -> usize;

    /// Writes bytes from `buf` and returns the number written.
    fn write(&self, buf: &[u8]) -> usize;
}

const BAUD_RATE: u32 = 115_200;

/// Returns the nearest 16550 integer divisor.
pub(crate) fn uart_divisor(clock_hz: u32) -> Option<u16> {
    let denominator = BAUD_RATE.checked_mul(16)?;
    let divisor = clock_hz.checked_add(denominator / 2)? / denominator;
    (1..=u16::MAX as u32)
        .contains(&divisor)
        .then_some(divisor as u16)
}

/// Returns the PL011 integer and six-bit fractional divisors.
pub(crate) fn pl011_divisors(clock_hz: u32) -> Option<(u16, u8)> {
    let denominator = BAUD_RATE.checked_mul(16)?;
    let mut integer = clock_hz / denominator;
    let remainder = clock_hz % denominator;
    let mut fraction = remainder.checked_mul(64)?.checked_add(denominator / 2)? / denominator;

    if fraction == 64 {
        integer = integer.checked_add(1)?;
        fraction = 0;
    }
    if !(1..=u16::MAX as u32).contains(&integer) || fraction > 63 {
        return None;
    }
    Some((integer as u16, fraction as u8))
}

pub(super) fn from_board(board: &BoardInfo) -> Option<Box<dyn ConsoleDevice>> {
    let info = board.console.as_ref()?;
    let base = info.base_address;
    let device: Box<dyn ConsoleDevice> = match info.kind {
        ConsoleKind::Uart16550Byte => {
            let mmio = Mmio::within(board, base, uart16550::BYTE_SPAN)?;
            Box::new(uart16550::Uart16550::new(mmio, false))
        }
        ConsoleKind::Uart16550Word => {
            let mmio = Mmio::within(board, base, uart16550::WORD_SPAN)?;
            Box::new(uart16550::Uart16550::new(mmio, true))
        }
        ConsoleKind::AxiLite => {
            let mmio = Mmio::within(board, base, axi_lite::SPAN)?;
            Box::new(axi_lite::UartAxiLite::new(mmio))
        }
        ConsoleKind::Bflb => {
            let mmio = Mmio::within(board, base, bflb::SPAN)?;
            Box::new(bflb::UartBflb::new(mmio))
        }
        ConsoleKind::Sifive => {
            let mmio = Mmio::within(board, base, sifive::SPAN)?;
            Box::new(sifive::UartSifive::new(mmio))
        }
        ConsoleKind::Pl011 => {
            let mmio = Mmio::within(board, base, pl011::SPAN)?;
            Box::new(pl011::UartPl011::new(mmio)?)
        }
        ConsoleKind::Xscale => {
            let mmio = Mmio::within(board, base, xscale::SPAN)?;
            Box::new(xscale::UartXscale::new(mmio, info.clock_hz)?)
        }
    };
    Some(device)
}
