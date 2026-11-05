//! Intel XScale/PXA UART.
//!
//! # References
//!
//! - Hardware manual: [Intel PXA27x Processor Family Developer's Manual, order 280000-001](https://support.eurotech-inc.com/developers/datasheets/PXA270_DeveloperManual.pdf),
//!   Chapter 10 — full-function UART register layout and fields.

mod description;
mod device;

use alloc::boxed::Box;
use core::mem::align_of;
use runtime::{Error, FdtNode, PlatformView, Result};

use super::ConsoleDesc;
use crate::devicetree::u32_property;

use self::{description::Desc, device::SPAN};

/// Parses an XScale UART node and reads its clock frequency.
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
    Ok(Some(Box::new(Desc {
        registers,
        clock_hz: u32_property(node, "clock-frequency"),
    })))
}
