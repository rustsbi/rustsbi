//! Allwinner V821 SoC description and custom-CSR interface.
//!
//! [`AllwinnerV821Soc`] is the zero-sized capability produced from the
//! V821 root compatible. The `csr` module applies V821 initialization and cache
//! policy through the CSR crate's Andes register definitions.

mod csr;
mod description;

pub use csr::{A27L2LineOperation, AndesStatusRegister, NoncacheableAlias, V821Csr};
pub use description::AllwinnerV821Soc;
