//! Allwinner/Sunxi watchdog reset drivers.
//!
//! V104 and V105 are vendor IP revision tags kept inside this driver family.
//! V104 uses the CFG/MODE watchdog sequence, while V105 exposes a dedicated
//! software-reset command.

mod v104;
mod v105;

use alloc::boxed::Box;

use super::registry::ResetDriver;

/// Returns unbound driver probes from newest to oldest.
///
/// Both implementations are compiled in, but only the probe matching the
/// Platform Description can claim MMIO and become a reset backend.
pub(super) fn built_in_drivers() -> [Box<dyn ResetDriver>; 2] {
    [
        Box::new(v105::V105Driver::default()),
        Box::new(v104::V104Driver::default()),
    ]
}
