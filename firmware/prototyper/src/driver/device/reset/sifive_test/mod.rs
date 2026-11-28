//! SiFive test finisher: board exit and reset signaling.
//!
//! # References
//!
//! - Platform source: [QEMU SiFive test-finisher implementation](https://github.com/qemu/qemu/blob/99e54ab5e7a6efc945af6d5661842155d1f3fc7a/hw/misc/sifive_test.c) —
//!   finisher register values and exit-code encoding.

mod description;
mod device;

use runtime::{FdtNode, PlatformView, Result};

use crate::devicetree::is_compatible;

use super::primary_registers;
pub(super) use description::Desc;
pub(super) use device::SifiveTestDevice;

/// Parses the SiFive test finisher without acquiring its MMIO region.
pub(super) fn describe(platform: &PlatformView<'_>, node: FdtNode<'_, '_>) -> Result<Option<Desc>> {
    if !is_compatible(node, "sifive,test0") {
        return Ok(None);
    }
    Ok(Some(Desc {
        registers: primary_registers(platform, node)?,
    }))
}
