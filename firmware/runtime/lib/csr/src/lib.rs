//! Typed RISC-V CSR access for RustSBI firmware.

#![no_std]

mod macros;
mod value;
mod standard;

pub use value::{Readable, Writeable};
pub use standard::isa::{
    privileged::{machine},
    values,
};
