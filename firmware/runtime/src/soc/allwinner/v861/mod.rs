//! Allwinner V861 SoC description and C907 custom-CSR interface.

mod csr;
mod description;
mod reset;

pub use description::{AllwinnerV861Soc, C907_HART_COUNT};
