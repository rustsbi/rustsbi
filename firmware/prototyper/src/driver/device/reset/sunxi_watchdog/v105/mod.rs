//! Allwinner V105 watchdog reset driver.
//!
//! The driver stops the watchdog before issuing its dedicated software-reset command.
//! When the Platform Description supplies an RTC V203 `gprcm_reg` window,
//! it also enables the associated reset gate.
//! Discovery and initialization do not modify hardware.
//! Runtime's reset service serializes the command sequence across harts.
//!
//! # References
//!
//! - Vendor implementation: [Tina V821 watchdog driver](https://github.com/sam-yangjj/tina-v821-v1.3-bsp/blob/4f82c3bed72342d306e1b14755f9618a6ca7a504/drivers/watchdog/sunxi_wdt.c)
//!   — V105 register offsets, update key, and stop-then-trigger ordering.

mod description;
mod device;

use runtime::{PlatformView, Result};

use super::super::{primary_registers, selection::find};
use description::Desc;

const COMPATIBLE: &str = "allwinner,wdt-v105";

/// Discovers the watchdog and its optional RTC gate without acquiring MMIO.
pub(in super::super) fn describe(platform: &PlatformView<'_>) -> Result<Option<Desc>> {
    let registers = find(platform, |discovered| {
        if discovered
            .compatible()
            .is_some_and(|compatibles| compatibles.all().any(|compatible| compatible == COMPATIBLE))
        {
            return primary_registers(platform, discovered.node()).map(Some);
        }
        Ok(None)
    })?;
    let Some(registers) = registers else {
        return Ok(None);
    };
    let reset_gate = find(platform, |node| platform.sunxi_rtc_v203_gprcm(node.node()))?;
    Ok(Some(Desc {
        registers,
        reset_gate,
    }))
}
