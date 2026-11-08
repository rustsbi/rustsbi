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

use self::{
    description::{DescU8, DescU32},
    device::{U8_SPAN, U32_SPAN},
};

/// Describes a byte-spaced 16550 when its properties accept byte access.
pub(super) fn describe_u8(
    platform: &PlatformView<'_>,
    node: FdtNode<'_, '_>,
) -> Result<Option<ConsoleDesc>> {
    if !accepts_u8(node) {
        return Ok(None);
    }
    let registers = platform
        .device_register(node)?
        .ok_or(Error::InvalidArgs)?
        .subrange(0, U8_SPAN)?;
    Ok(Some(Box::new(DescU8 { registers })))
}

/// Describes a word-spaced 16550 after checking its access properties.
pub(super) fn describe_u32(
    platform: &PlatformView<'_>,
    node: FdtNode<'_, '_>,
    compatible: &str,
) -> Result<Option<ConsoleDesc>> {
    if !accepts_u32(node, compatible) {
        return Ok(None);
    }
    let registers = platform
        .device_register(node)?
        .ok_or(Error::InvalidArgs)?
        .subrange(0, U32_SPAN)?;
    if !registers.has_aligned_bounds(align_of::<u32>()) {
        return Err(Error::InvalidArgs);
    }
    Ok(Some(Box::new(DescU32 { registers })))
}

/// Returns whether the node describes byte-spaced registers.
fn accepts_u8(node: FdtNode<'_, '_>) -> bool {
    u32_property(node, "reg-shift").unwrap_or(0) == 0
        && u32_property(node, "reg-io-width").unwrap_or(1) == 1
}

/// Returns whether the node describes word-spaced registers.
///
/// For DesignWare and Allwinner compatibles, this driver assumes 32-bit access.
/// Other nodes must declare both `reg-shift = 2` and `reg-io-width = 4`.
fn accepts_u32(node: FdtNode<'_, '_>, compatible: &str) -> bool {
    matches!(compatible, "snps,dw-apb-uart" | "allwinner,sunxi-uart")
        || (u32_property(node, "reg-shift") == Some(2)
            && u32_property(node, "reg-io-width") == Some(4))
}
