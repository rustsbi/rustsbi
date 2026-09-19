//! Allwinner V861 SoC description and C907 custom-CSR interface.

mod csr;
mod description;

pub use csr::C907CacheState;
pub use description::{AllwinnerV861Soc, C907_HART_COUNT};
