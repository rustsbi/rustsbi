//! V821 USB DMA bypass description.

use runtime::memory::RegRange;

pub(crate) struct Desc {
    pub(crate) registers: RegRange,
}
