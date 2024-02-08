//! Simple RISC-V SBI runtime primitives.
#![no_std]
#[cfg_attr(not(feature = "legacy"), deny(missing_docs))]
// §3
mod binary;
// §4
mod base;
// §5
#[cfg(feature = "legacy")]
pub mod legacy;
// §6
mod time;
// §7
mod spi;
// §8
mod rfnc;
// §9
mod hsm;
// §10
mod srst;
// §11
mod pmu;
// §12
mod dbcn;
// §13
mod susp;
// §14
mod cppc;
// §15
mod nacl;
// §16
mod sta;

pub use sbi_spec::{
    base::Version,
    binary::{HartMask, Physical, SbiRet, SharedPtr},
};

// module `binary` includes crate-local `sbi_call_*` functions and is thus not re-exported
// into the library root.

pub use base::*;
pub use cppc::*;
pub use dbcn::*;
pub use hsm::*;
pub use nacl::*;
pub use pmu::*;
pub use rfnc::*;
pub use spi::*;
pub use srst::*;
pub use sta::*;
pub use susp::*;
pub use time::*;
