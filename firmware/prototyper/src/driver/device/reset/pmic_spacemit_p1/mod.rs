//! SpacemiT P1 PMIC reset driver.
//!
//! # References
//!
//! - Reference implementation: [Linux P1 reboot driver](https://github.com/torvalds/linux/blob/2687c848e57820651b9f69d30c4710f4219f7dbf/drivers/power/reset/spacemit-p1-reboot.c)
//!   — power-control register and reset/shutdown bits used by K1 systems.

mod controller;
mod description;
mod device;

use runtime::{Error, PlatformView, Result};

use crate::devicetree::EnabledNode;

use super::primary_registers;
use description::{Desc, I2cAddress};

const PMIC_COMPATIBLES: [&str; 2] = ["spacemit,p1", "ky,spm8821"];
const I2C_COMPATIBLES: [&str; 2] = ["spacemit,k1-i2c", "ky,i2c"];

/// Describes a PMIC and its parent I2C controller without acquiring MMIO.
pub(super) fn describe(
    platform: &PlatformView<'_>,
    discovered: EnabledNode<'_, '_>,
    timebase_frequency_hz: Option<u32>,
) -> Result<Option<Desc>> {
    let Some(compatibles) = discovered.compatible() else {
        return Ok(None);
    };
    if !compatibles
        .all()
        .any(|compatible| PMIC_COMPATIBLES.contains(&compatible))
    {
        return Ok(None);
    }
    let node = discovered.node();

    // A PMIC child's `reg` value is an address on its parent I2C bus, not a
    // physical MMIO range, so it must not be parsed as one.
    let mut address_entries = node.reg().ok_or(Error::InvalidArgs)?;
    let address_entry = address_entries.next().ok_or(Error::InvalidArgs)?;
    if address_entries.next().is_some() {
        return Err(Error::InvalidArgs);
    }
    let address =
        I2cAddress::new(address_entry.starting_address as usize).ok_or(Error::InvalidArgs)?;

    let parent = discovered.parent().ok_or(Error::InvalidArgs)?;
    let parent_compatibles = parent.compatible().ok_or(Error::InvalidArgs)?;
    if !parent_compatibles
        .all()
        .any(|compatible| I2C_COMPATIBLES.contains(&compatible))
    {
        return Err(Error::InvalidArgs);
    }
    let controller = controller::Desc {
        registers: primary_registers(platform, parent)?,
        timebase_frequency_hz,
    };
    Ok(Some(Desc {
        controller,
        address,
    }))
}
