//! Allwinner V105 watchdog reset driver.
//!
//! Reset requests can enable a gate in the RTC V203 `gprcm_reg` window.
//! Discovery and initialization leave the watchdog and reset gate untouched.
//!
//! # References
//!
//! - Vendor implementation: [Tina V821 watchdog driver](https://github.com/sam-yangjj/tina-v821-v1.3-bsp/blob/4f82c3bed72342d306e1b14755f9618a6ca7a504/drivers/watchdog/sunxi_wdt.c)
//!   — V105 register offsets, update key, and stop-then-trigger ordering.

mod description;
mod device;

use runtime::{PlatformView, Result};

use crate::devicetree::is_compatible;

use super::super::{primary_registers, selection::find};
pub(in super::super) use description::Desc;
pub(in super::super) use device::V105Watchdog;

const COMPATIBLE: &str = "allwinner,wdt-v105";

/// Discovers the watchdog and its optional RTC reset gate.
pub(in super::super) fn describe(platform: &PlatformView<'_>) -> Result<Option<Desc>> {
    let registers = find(platform, |node, _| {
        if is_compatible(node, COMPATIBLE) {
            return primary_registers(platform, node).map(Some);
        }
        Ok(None)
    })?;
    let Some(registers) = registers else {
        return Ok(None);
    };
    let reset_gate = find(platform, |node, _| platform.sunxi_rtc_v203_gprcm(node))?;
    Ok(Some(Desc {
        registers,
        reset_gate,
    }))
}
