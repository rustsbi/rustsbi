//! Allwinner V104 watchdog reset driver.
//!
//! Runtime acquires the register window once.
//! The SBI reset mutex serializes calls through this driver only.
//! Platform integration must ensure that no other firmware
//! or supervisor watchdog driver programs the same registers.
//! The loader must leave the watchdog clock and architectural time counter running.
//!
//! # References
//!
//! - Reference implementation: [F101 OpenSBI platform](https://github.com/YuzukiHD/opensbi/blob/08cbbe0bf6d2aaf0e8dbef8c548f4befdf3b7a7e/platform/generic/allwinner/sun252i-f101.c)
//!   — CFG/MODE sequence, update key, and required one-millisecond delay.

mod description;
mod device;

use runtime::{PlatformView, Result};

use crate::devicetree::EnabledNode;

use super::super::primary_registers;
use description::Desc;

const COMPATIBLES: [&str; 2] = ["allwinner,sun20i-d1-wdt", "allwinner,wdt-v104"];

/// Parses the watchdog registers without acquiring their MMIO region.
pub(in super::super) fn describe(
    platform: &PlatformView<'_>,
    discovered: EnabledNode<'_, '_>,
    timebase_frequency_hz: Option<u32>,
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
    let registers = primary_registers(platform, discovered.node())?;
    Ok(Some(Desc {
        registers,
        timebase_frequency_hz,
    }))
}
