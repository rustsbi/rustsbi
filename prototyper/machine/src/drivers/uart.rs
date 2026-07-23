//! Validated UART bindings retained by Prototyper.

use alloc::boxed::Box;
use core::ops::Range;

use dtoolkit::fdt::{Fdt, FdtNode};
use dtoolkit::{Node, Property};

use super::DriverError;
use crate::boot::BootInfo;
use crate::boot::device_tree::{BindingError, enabled, exact_node, model, reg_ranges};
use crate::config::TRUSTED_TARGET;
use crate::console::{Console, ConsoleDevice, ConsoleError};
use crate::memory::IoMem;

mod arch;

use arch::io_fence;

const QEMU_MODEL: &str = "riscv-virtio,qemu";
const QEMU_UART_BASE: usize = 0x1000_0000;
const UART16550_U8: &str = "ns16550a";
const UART16550_U32: &str = "snps,dw-apb-uart";
const UART_AXI_LITE: &str = "xlnx,xps-uartlite-1.00.a";
const UART_BFLB: &str = "bflb,bl808-uart";
const UART_SIFIVE: &str = "sifive,uart0";
const UART_PL011: &str = "pl011";

/// Validates, claims, and binds the selected firmware UART exactly once.
pub fn build(boot: &mut BootInfo, node_path: &str) -> Result<Console, DriverError> {
    let binding = Binding::from_dtb(boot, node_path)?;
    let io = IoMem::acquire(boot, binding.range).map_err(|error| match error {
        crate::memory::IoMemError::InvalidRange => DriverError::InvalidRange,
        crate::memory::IoMemError::AlreadyClaimed => DriverError::AlreadyOwned,
        crate::memory::IoMemError::OutOfBounds | crate::memory::IoMemError::Misaligned => {
            DriverError::InvalidRange
        }
    })?;
    let uart = Uart {
        io,
        kind: binding.kind,
    };
    uart.initialize();
    Ok(Console::new(Box::new(uart)))
}

struct Binding {
    range: Range<usize>,
    kind: Kind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Kind {
    Uart16550 { stride: usize },
    AxiLite,
    Bflb,
    SiFive,
    Pl011,
}

impl Kind {
    const fn required_size(self) -> usize {
        match self {
            Self::Uart16550 { stride } => 6 * stride,
            Self::AxiLite => 0x0c,
            Self::Bflb => 0x90,
            Self::SiFive => 0x1c,
            Self::Pl011 => 0x48,
        }
    }

    const fn word_access(self) -> bool {
        !matches!(self, Self::Uart16550 { stride: 1 })
    }
}

impl Binding {
    fn from_dtb(boot: &BootInfo, path: &str) -> Result<Self, DriverError> {
        let fdt = Fdt::new(boot.dtb().as_bytes()).map_err(|_| DriverError::DeviceTree)?;
        let node = exact_node(&fdt, path).map_err(map_binding_error)?;
        if !enabled(&node) {
            return Err(DriverError::Unsupported);
        }
        let kind = kind(&node).ok_or(DriverError::Unsupported)?;
        let ranges = reg_ranges(node).map_err(map_binding_error)?;
        if ranges.len() != 1 {
            return Err(DriverError::InvalidRange);
        }
        let range = ranges.into_iter().next().ok_or(DriverError::InvalidRange)?;
        let required_end = range
            .start
            .checked_add(kind.required_size())
            .ok_or(DriverError::InvalidRange)?;
        if required_end > range.end || (kind.word_access() && !range.start.is_multiple_of(4)) {
            return Err(DriverError::InvalidRange);
        }

        let canonical_qemu = model(&fdt) == QEMU_MODEL
            && range.start == QEMU_UART_BASE
            && kind == (Kind::Uart16550 { stride: 1 });
        if !canonical_qemu && !TRUSTED_TARGET {
            return Err(DriverError::Unauthorized);
        }
        Ok(Self { range, kind })
    }
}

fn kind(node: &FdtNode<'_>) -> Option<Kind> {
    node.property("compatible")?
        .as_str_list()
        .find_map(|compatible| match compatible {
            UART16550_U8 => Some(Kind::Uart16550 { stride: 1 }),
            UART16550_U32 => Some(Kind::Uart16550 { stride: 4 }),
            UART_AXI_LITE => Some(Kind::AxiLite),
            UART_BFLB => Some(Kind::Bflb),
            UART_SIFIVE => Some(Kind::SiFive),
            UART_PL011 => Some(Kind::Pl011),
            _ => None,
        })
}

fn map_binding_error(error: BindingError) -> DriverError {
    match error {
        BindingError::DeviceTree => DriverError::DeviceTree,
        BindingError::Unsupported => DriverError::Unsupported,
        BindingError::InvalidRange => DriverError::InvalidRange,
    }
}

struct Uart {
    io: IoMem,
    kind: Kind,
}

impl Uart {
    fn initialize(&self) {
        io_fence();
        match self.kind {
            Kind::SiFive => {
                self.write_u32(0x10, 0);
                self.write_u32(0x0c, self.read_u32(0x0c) | 1);
                self.write_u32(0x08, self.read_u32(0x08) | 1);
            }
            Kind::Pl011 => {
                // Preserve the previous implementation's fixed 24 MHz,
                // 115200-baud, 8-N-1 setup. A future clock-provider binding
                // should replace these constants before adding new boards.
                self.write_u32(0x30, 0);
                self.write_u32(0x44, 0x7ff);
                self.write_u32(0x24, 13);
                self.write_u32(0x28, 1);
                self.write_u32(0x2c, (0b11 << 5) | (1 << 4));
                self.write_u32(0x38, 0);
                self.write_u32(0x30, (1 << 9) | (1 << 8) | 1);
            }
            Kind::Uart16550 { .. } | Kind::AxiLite | Kind::Bflb => {}
        }
        io_fence();
    }

    fn read_u8(&self, offset: usize) -> u8 {
        self.io
            .read_once(offset)
            .unwrap_or_else(|_| crate::power::abort(|| {}))
    }

    fn write_u8(&self, offset: usize, value: u8) {
        self.io
            .write_once(offset, value)
            .unwrap_or_else(|_| crate::power::abort(|| {}))
    }

    fn read_u32(&self, offset: usize) -> u32 {
        self.io
            .read_once(offset)
            .unwrap_or_else(|_| crate::power::abort(|| {}))
    }

    fn write_u32(&self, offset: usize, value: u32) {
        self.io
            .write_once(offset, value)
            .unwrap_or_else(|_| crate::power::abort(|| {}))
    }

    fn read_16550(&self, index: usize, stride: usize) -> u8 {
        if stride == 1 {
            self.read_u8(index)
        } else {
            self.read_u32(index * stride) as u8
        }
    }

    fn write_16550(&self, index: usize, stride: usize, value: u8) {
        if stride == 1 {
            self.write_u8(index, value);
        } else {
            self.write_u32(index * stride, u32::from(value));
        }
    }
}

impl ConsoleDevice for Uart {
    fn read(&mut self, destination: &mut [u8]) -> Result<usize, ConsoleError> {
        io_fence();
        let count = match self.kind {
            Kind::Uart16550 { stride } => read_while(destination, || {
                (self.read_16550(5, stride) & 1 != 0).then(|| self.read_16550(0, stride))
            }),
            Kind::AxiLite => read_while(destination, || {
                (self.read_u32(0x08) & 1 != 0).then(|| self.read_u32(0) as u8)
            }),
            Kind::Bflb => read_while(destination, || {
                (self.read_u32(0x84) >> 8 & 0x3f != 0).then(|| self.read_u32(0x8c) as u8)
            }),
            Kind::SiFive => read_while(destination, || {
                let value = self.read_u32(0x04);
                (value >> 31 == 0).then_some(value as u8)
            }),
            Kind::Pl011 => read_while(destination, || {
                (self.read_u32(0x18) & (1 << 4) == 0).then(|| self.read_u32(0) as u8)
            }),
        };
        io_fence();
        Ok(count)
    }

    fn write(&mut self, source: &[u8]) -> Result<usize, ConsoleError> {
        io_fence();
        let mut count = 0;
        for &byte in source {
            let accepted = match self.kind {
                Kind::Uart16550 { stride } if self.read_16550(5, stride) & (1 << 5) != 0 => {
                    self.write_16550(0, stride, byte);
                    true
                }
                Kind::AxiLite if self.read_u32(0x08) & (1 << 3) == 0 => {
                    self.write_u32(0x04, u32::from(byte));
                    true
                }
                Kind::Bflb if self.read_u32(0x84) & 0x3f != 0 => {
                    self.write_u32(0x88, u32::from(byte));
                    true
                }
                Kind::SiFive if self.read_u32(0) >> 31 == 0 => {
                    self.write_u32(0, u32::from(byte));
                    true
                }
                Kind::Pl011 if self.read_u32(0x18) & (1 << 5) == 0 => {
                    self.write_u32(0, u32::from(byte));
                    true
                }
                Kind::Uart16550 { .. }
                | Kind::AxiLite
                | Kind::Bflb
                | Kind::SiFive
                | Kind::Pl011 => false,
            };
            if !accepted {
                break;
            }
            count += 1;
        }
        io_fence();
        Ok(count)
    }
}

fn read_while(destination: &mut [u8], mut read: impl FnMut() -> Option<u8>) -> usize {
    let mut count = 0;
    for byte in destination {
        let Some(value) = read() else {
            break;
        };
        *byte = value;
        count += 1;
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_register_window_includes_its_last_access() {
        assert_eq!(Kind::Uart16550 { stride: 1 }.required_size(), 6);
        assert_eq!(Kind::Uart16550 { stride: 4 }.required_size(), 0x18);
        assert_eq!(Kind::AxiLite.required_size(), 0x0c);
        assert_eq!(Kind::Bflb.required_size(), 0x90);
        assert_eq!(Kind::SiFive.required_size(), 0x1c);
        assert_eq!(Kind::Pl011.required_size(), 0x48);
    }

    #[test]
    fn only_byte_stride_16550_avoids_word_alignment() {
        assert!(!Kind::Uart16550 { stride: 1 }.word_access());
        assert!(Kind::Uart16550 { stride: 4 }.word_access());
        assert!(Kind::Pl011.word_access());
    }
}
