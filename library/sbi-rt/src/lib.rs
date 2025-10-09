//! RISC-V SBI runtime primitives library.
//!
//! `sbi-rt` provides fundamental runtime primitives for the RISC-V Supervisor Binary
//! Interface (SBI), wrapping low-level SBI calls in safe Rust interfaces that return
//! `SbiRet` results.
//!
//! All the `SbiRet` types returned by SBI call functions of this crate are `SbiRet<usize>`,
//! representing the pointer width of the current RISC-V SBI platform.
//! Those calls only works at RISC-V targets when building supervisor software
//! (e.g. kernels or hypervisors); it builds under non-RISC-V targets but for tests
//! or `cargo fix` purposes only.
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
// §17
mod sse;
// §18
mod fwft;
// §19
mod dbtr;
// §20
mod mpxy;

pub use sbi_spec::{
    base::Version,
    binary::{CounterMask, HartMask, Physical, SbiRet, SharedPtr},
};

// module `binary` includes crate-local `sbi_call_*` functions and is thus not re-exported
// into the library root.

pub use base::*;
pub use cppc::*;
pub use dbcn::*;
pub use dbtr::*;
pub use fwft::*;
pub use hsm::*;
pub use mpxy::*;
pub use nacl::*;
pub use pmu::*;
pub use rfnc::*;
pub use spi::*;
pub use srst::*;
pub use sse::*;
pub use sta::*;
pub use susp::*;
pub use time::*;

/// Raw RISC-V SBI calls.
///
/// This module is not intended for direct use; it should be used by runtime libraries for custom
/// SBI extensions, to wrap SBI `ecall` instruction into Rust friendly functions.
///
/// SBI runtime users should use functions from root of this library, or functions from other SBI
/// runtime libraries.
pub mod raw {
    use sbi_spec::binary::SbiRet;

    /// Raw SBI call with 0 parameters.
    #[inline(always)]
    pub unsafe fn sbi_call_0(eid: usize, fid: usize) -> SbiRet {
        super::binary::sbi_call_0(eid, fid)
    }

    /// Raw SBI call with 1 parameter.
    #[inline(always)]
    pub unsafe fn sbi_call_1(eid: usize, fid: usize, arg0: usize) -> SbiRet {
        super::binary::sbi_call_1(eid, fid, arg0)
    }

    /// Raw SBI call with 2 parameters.
    #[inline(always)]
    pub unsafe fn sbi_call_2(eid: usize, fid: usize, arg0: usize, arg1: usize) -> SbiRet {
        super::binary::sbi_call_2(eid, fid, arg0, arg1)
    }

    // TODO sbi_call_3, ..., sbi_call_6
}
