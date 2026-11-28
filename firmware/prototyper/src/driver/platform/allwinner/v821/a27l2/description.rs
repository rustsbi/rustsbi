//! V821 A27L2 cache description.

use runtime::{memory::RegRange, soc::allwinner::v821::V821Csr};

pub(crate) struct Desc {
    pub(crate) csr: V821Csr,
    pub(crate) registers: RegRange,
}
