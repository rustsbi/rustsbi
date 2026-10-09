//! Allwinner SoC-specific firmware drivers.
//!
//! Each chip module owns its device register protocols. Platform modules
//! select these devices and determine their initialization order.

pub(crate) mod v821;
pub(crate) mod v861;
