//! Devicetree description of the Intel XScale/PXA UART.

use runtime::memory::RegRange;

/// Parsed resources of one XScale UART instance.
pub(crate) struct Desc {
    pub(super) registers: RegRange,
    pub(super) clock_hz: Option<u32>,
}
