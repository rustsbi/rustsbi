//! Devicetree description of the Arm PL011 UART.

use runtime::memory::RegRange;

/// Parsed resources of one PL011 UART instance.
pub(crate) struct Desc {
    pub(super) registers: RegRange,
    pub(super) clock_hz: Option<u32>,
}
