//! SpacemiT P1 PMIC reset driver.
//!
//! # References
//!
//! - Reference implementation: [Linux P1 reboot driver](https://github.com/torvalds/linux/blob/2687c848e57820651b9f69d30c4710f4219f7dbf/drivers/power/reset/spacemit-p1-reboot.c)
//!   — power-control register and reset/shutdown bits used by K1 systems.

mod controller;
mod description;
mod device;

use runtime::{Error, FdtNode, PlatformView, Result};

use crate::devicetree::u32_property;

use super::primary_registers;
pub(super) use description::Desc;
use description::I2cAddress;
pub(super) use device::P1Pmic;

const PMIC_COMPATIBLES: [&str; 2] = ["spacemit,p1", "ky,spm8821"];
const I2C_COMPATIBLES: [&str; 2] = ["spacemit,k1-i2c", "ky,i2c"];

/// Describes a PMIC and its parent I2C controller without acquiring MMIO.
pub(super) fn describe(
    platform: &PlatformView<'_>,
    node: FdtNode<'_, '_>,
    parent: Option<FdtNode<'_, '_>>,
    timebase_frequency_hz: Option<u32>,
) -> Result<Option<Desc>> {
    let Some(compatibles) = node.compatible() else {
        return Ok(None);
    };
    if !compatibles
        .all()
        .any(|compatible| PMIC_COMPATIBLES.contains(&compatible))
    {
        return Ok(None);
    }

    // The I2C binding encodes the device address in one cell.
    let address = u32_property(node, "reg")
        .and_then(I2cAddress::new)
        .ok_or(Error::InvalidArgs)?;

    let parent = parent.ok_or(Error::InvalidArgs)?;
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
