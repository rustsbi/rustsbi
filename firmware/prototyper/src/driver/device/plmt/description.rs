//! Devicetree description of an Andes PLMT.

use runtime::memory::RegRange;

/// A PLMT register window before acquisition.
pub(crate) struct Desc {
    pub(crate) registers: RegRange,
}
