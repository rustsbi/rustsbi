//! Allwinner/Sunxi watchdog reset drivers.
//!
//! V104 and V105 are vendor IP revision tags. V104 uses the CFG/MODE watchdog
//! sequence, while V105 exposes a dedicated software-reset command.

mod v104;
mod v105;

pub(super) use v104::WATCHDOG_V104;
pub(super) use v105::WATCHDOG_V105;
