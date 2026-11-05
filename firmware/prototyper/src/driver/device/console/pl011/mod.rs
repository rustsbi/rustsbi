//! Arm PrimeCell UART (PL011).
//!
//! # References
//!
//! - Hardware manual: [Arm DDI 0183G, *PrimeCell UART (PL011) Technical Reference Manual*](https://developer.arm.com/documentation/ddi0183/g/),
//!   Programmer's Model — register layout, fields, and baud-rate divisors.

mod description;
mod device;

use alloc::boxed::Box;
use core::mem::align_of;
use runtime::{Error, FdtNode, PlatformView, Result};

use super::ConsoleDesc;
use crate::devicetree::u32_property;

use self::{description::Desc, device::SPAN};

/// Parses a PL011 node and reads its clock frequency.
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
