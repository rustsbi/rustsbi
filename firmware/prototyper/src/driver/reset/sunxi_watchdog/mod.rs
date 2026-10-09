//! Allwinner/Sunxi watchdog reset drivers.
//!
//! V104 and V105 are vendor IP revision tags kept inside this driver family.
//! V104 uses the CFG/MODE watchdog sequence, while V105 exposes a dedicated
//! software-reset command.

mod v104;
mod v105;

pub(super) use v104::V104Driver;
pub(super) use v105::V105Driver;
