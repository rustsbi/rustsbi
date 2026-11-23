//! Xilinx AXI UART Lite.
//!
//! # References
//!
//! - Hardware manual: [AMD PG142, *AXI UART Lite v2.0 Product Guide*](https://docs.amd.com/v/u/en-US/pg142-axi-uartlite) —
//!   register layout and status/control fields.

mod description;
mod device;

use core::mem::align_of;
use runtime::{Error, FdtNode, PlatformView, Result};

use super::ConsoleDesc;

use self::device::SPAN;
pub(super) use description::Desc;
pub(super) use device::Device;

/// Parses an AXI UART Lite node and validates its register window.
pub(super) fn describe(
    platform: &PlatformView<'_>,
    node: FdtNode<'_, '_>,
) -> Result<Option<ConsoleDesc>> {
    let registers = platform
        .device_register(node)?
        .ok_or(Error::InvalidArgs)?
        .subrange(0, SPAN)?;
    if !registers.has_aligned_bounds(align_of::<u32>()) {
        return Err(Error::InvalidArgs);
    }
    Ok(Some(ConsoleDesc::AxiLite(Desc { registers })))
}
