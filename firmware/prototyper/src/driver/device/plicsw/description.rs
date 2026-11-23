//! Devicetree description of an Allwinner PLICSW.

use runtime::memory::RegRange;

/// A PLICSW register window before acquisition.
pub(crate) struct Desc {
    pub(crate) registers: RegRange,
}
