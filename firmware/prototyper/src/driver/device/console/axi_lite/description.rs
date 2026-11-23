//! Devicetree description of the Xilinx AXI UART Lite.

use runtime::memory::RegRange;

/// Parsed resources of one AXI UART Lite instance.
pub(crate) struct Desc {
    pub(super) registers: RegRange,
}
