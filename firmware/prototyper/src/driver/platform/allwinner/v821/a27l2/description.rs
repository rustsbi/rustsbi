//! V821 A27L2 cache description.

use runtime::{memory::RegRange, soc::allwinner::v821::AllwinnerV821Soc};

pub(crate) struct Desc {
    pub(crate) soc: AllwinnerV821Soc,
    pub(crate) registers: RegRange,
}
