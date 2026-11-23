//! Drivers for devices shared by multiple platforms.

pub(crate) mod aclint;
pub(crate) mod cci;
pub(crate) mod clint;
pub(crate) mod console;
mod hart_slot;
pub(crate) mod imsic;
pub(crate) mod interrupts;
pub(crate) mod mswi;
pub(crate) mod mtimer;
pub(crate) mod plicsw;
pub(crate) mod plmt;
pub(crate) mod reset;
pub(crate) mod thead_plic;

use super::{Description, match_desc};
use runtime::{Error, Result};

/// Records a device description and rejects duplicates.
fn select_once<T>(slot: &mut Option<T>, value: T) -> Result<()> {
    if slot.is_some() {
        return Err(Error::InvalidArgs);
    }
    *slot = Some(value);
    Ok(())
}
