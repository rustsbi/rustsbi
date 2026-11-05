//! SiFive test finisher: board exit and reset signaling.
//!
//! # References
//!
//! - Platform source: [QEMU SiFive test-finisher implementation](https://github.com/qemu/qemu/blob/99e54ab5e7a6efc945af6d5661842155d1f3fc7a/hw/misc/sifive_test.c) —
//!   finisher register values and exit-code encoding.

mod description;
mod device;

use runtime::{PlatformView, Result};

use crate::devicetree::EnabledNode;

use super::primary_registers;
use description::Desc;

const COMPATIBLES: [&str; 1] = ["sifive,test0"];

/// Parses the SiFive test finisher without acquiring its MMIO region.
pub(super) fn describe(
    platform: &PlatformView<'_>,
    discovered: EnabledNode<'_, '_>,
) -> Result<Option<Desc>> {
    let Some(compatibles) = discovered.compatible() else {
        return Ok(None);
    };
    if !compatibles
        .all()
        .any(|compatible| COMPATIBLES.contains(&compatible))
    {
        return Ok(None);
    }
    Ok(Some(Desc {
        registers: primary_registers(platform, discovered.node())?,
    }))
}
