//! Description of the SiFive test finisher.

use runtime::memory::RegRange;

/// Validated reset registers whose MMIO region has not been acquired yet.
pub(crate) struct Desc {
    pub(super) registers: RegRange,
}
