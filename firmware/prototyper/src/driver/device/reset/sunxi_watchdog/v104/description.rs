//! Description of the Sunxi V104 watchdog.

use runtime::memory::RegRange;

/// Validated watchdog registers whose MMIO region has not been acquired yet.
pub(crate) struct Desc {
    pub(super) registers: RegRange,
    pub(super) timebase_frequency_hz: Option<u32>,
}
