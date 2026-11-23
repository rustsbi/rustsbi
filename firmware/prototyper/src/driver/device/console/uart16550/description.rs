//! Devicetree description of NS16550-compatible UARTs.

use runtime::memory::RegRange;

/// Parsed resources of one byte-spaced 16550 UART.
pub(crate) struct DescU8 {
    pub(super) registers: RegRange,
}

/// Parsed resources of one word-spaced 16550 UART.
pub(crate) struct DescU32 {
    pub(super) registers: RegRange,
}
