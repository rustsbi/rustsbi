//! Devicetree description of the SiFive UART.

use runtime::memory::RegRange;

/// Parsed resources of one SiFive UART instance.
pub(crate) struct Desc {
    pub(super) registers: RegRange,
}
