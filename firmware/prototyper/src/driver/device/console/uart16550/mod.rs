//! NS16550-compatible UART with byte- or word-spaced registers.
//!
//! # References
//!
//! - Hardware manual: [Texas Instruments TL16C550D data sheet](https://www.ti.com/lit/ds/symlink/tl16c550d.pdf),
//!   Register Functional Description — register indices and line-status fields.

mod description;
mod device;

use alloc::boxed::Box;
use core::mem::align_of;
use runtime::{Error, FdtNode, PlatformView, Result};

use super::ConsoleDesc;
use crate::devicetree::u32_property;

use self::device::{U8_SPAN, U32_SPAN};
use description::{DescU8, DescU32};

/// Describes a 16550 UART using its register access properties.
pub(super) fn describe(
    platform: &PlatformView<'_>,
    node: FdtNode<'_, '_>,
    compatible: &str,
) -> Result<Option<ConsoleDesc>> {
    // These compatibles select word access without consulting the properties.
    let word_access = if matches!(compatible, "snps,dw-apb-uart" | "allwinner,sunxi-uart") {
        true
    } else {
        match (
            u32_property(node, "reg-shift"),
            u32_property(node, "reg-io-width"),
        ) {
            (None | Some(0), None | Some(1)) => false,
            (Some(2), Some(4)) => true,
            _ => return Ok(None),
        }
    };
    let span = if word_access { U32_SPAN } else { U8_SPAN };
    let registers = platform
        .device_register(node)?
        .ok_or(Error::InvalidArgs)?
        .subrange(0, span)?;
    let description: ConsoleDesc = if word_access {
        if !registers.has_aligned_bounds(align_of::<u32>()) {
            return Err(Error::InvalidArgs);
        }
        Box::new(DescU32 { registers })
    } else {
        Box::new(DescU8 { registers })
    };
    Ok(Some(description))
}
