//! Bouffalo Lab BL808 UART.
//!
//! # References
//!
//! - Hardware manual: [BL808 Reference Manual v1.3](https://github.com/bouffalolab/bl_docs/blob/665e7daa84b498a767e656f458f26aca3be4fd45/BL808_RM/en/BL808_RM_en_1.3.pdf),
//!   UART chapter — register layout and FIFO count fields.

mod description;
mod device;

use alloc::boxed::Box;
use core::mem::align_of;
use runtime::{Error, FdtNode, PlatformView, Result};

use super::ConsoleDesc;

use self::{description::Desc, device::SPAN};

/// Parses a BL808 UART node and validates its register window.
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
    Ok(Some(Box::new(Desc { registers })))
}
