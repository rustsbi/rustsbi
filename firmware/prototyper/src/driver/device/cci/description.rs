//! CCI-550 register windows selected by the platform.

use runtime::memory::RegRange;

/// A platform-selected CCI-550 instance and its cluster interfaces.
pub(crate) struct Desc<const N: usize> {
    pub(crate) registers: RegRange,
    /// Zero-based slave-interface indices.
    pub(crate) interfaces: [usize; N],
}
