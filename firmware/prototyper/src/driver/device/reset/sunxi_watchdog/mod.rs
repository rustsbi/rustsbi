//! Allwinner/Sunxi watchdog reset drivers.
//!
//! V104 and V105 are vendor IP revision tags kept inside this driver family.
//! V104 uses the CFG/MODE watchdog sequence,
//! while V105 exposes a dedicated software-reset command.

pub(super) mod v104;
pub(super) mod v105;
