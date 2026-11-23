//! Description of the Sunxi V105 watchdog and its optional RTC gate.

use runtime::memory::RegRange;

/// Validated watchdog registers and an optional reset gate.
///
/// Neither MMIO region has been acquired yet.
pub(crate) struct Desc {
    pub(super) registers: RegRange,
    pub(super) reset_gate: Option<RegRange>,
}
