//! Devicetree description of the Bouffalo Lab BL808 UART.

use runtime::memory::RegRange;

/// Parsed resources of one BL808 UART instance.
pub(crate) struct Desc {
    pub(super) registers: RegRange,
}
