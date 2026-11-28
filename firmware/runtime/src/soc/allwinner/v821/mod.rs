//! Allwinner V821 SoC description and custom-CSR interface.
//!
//! [`AllwinnerV821Soc`] identifies a matching device-tree root. RV32 firmware
//! can use its Andes PMA and cache controls after confirming the hardware.

mod description;

mod csr;
pub use csr::{A27L2CacheControl, A27L2LineOperation, AndesStatusRegister, NoncacheableAlias};
pub use description::AllwinnerV821Soc;
