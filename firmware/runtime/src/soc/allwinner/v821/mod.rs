//! Allwinner V821 SoC description and custom-CSR interface.
//!
//! [`AllwinnerV821Soc`] is the zero-sized capability produced from the
//! V821 root compatible. The `csr` module contains the Andes custom CSRs
//! required by the V821 BSP; those registers are not standard RISC-V CSRs.

mod csr;
mod description;

pub use csr::{A27L2LineOperation, AndesStatusRegister, NoncacheableAlias, V821Csr};
pub use description::AllwinnerV821Soc;
