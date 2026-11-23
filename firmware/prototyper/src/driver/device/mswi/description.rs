//! Parsed machine software-interrupt resources.

use runtime::memory::RegRange;

use super::super::hart_slot::HartSlotMap;

/// A machine software-interrupt register window before acquisition.
pub(crate) struct Desc {
    pub(crate) registers: RegRange,
    pub(crate) hart_slots: HartSlotMap,
}
