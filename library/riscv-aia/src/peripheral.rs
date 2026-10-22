//! Memory-mapped peripherals for RISC-V AIA.

pub mod aplic;
pub mod imsic;

pub use aplic::{Aplic, Idc};
pub use imsic::Imsic;
