//! SpacemiT K1 and K3 custom RISC-V CSRs.
//!
//! Register names, fields, and access semantics are inferred from public
//! firmware because SpacemiT has not published a complete CSR specification.
//!
//! # Safety
//!
//! Callers must identify the current core and chip revision before accessing a
//! custom CSR. The generated `try_read` and `try_write` functions do not catch
//! illegal-instruction traps from CSRs that are absent on a RISC-V hart.
//!
//! For partially documented read-write registers, modify a value returned by
//! `read` before passing it to `write`. `from_bits` discards unmodeled bits, so
//! writing a newly constructed value can clear undocumented writable fields.
//! Cache, L2, and PMA writes can also change coherency or memory attributes
//! immediately.

pub mod tcmcfg; // 0x5db

pub mod mhcr; // 0x7c1
// mod mhint reuses Xuantie series.
pub mod mraop; // 0x7c2
pub mod msetup; // 0x7c0

#[cfg(target_pointer_width = "64")]
pub mod perf_ctrl; // 0x7d0
#[cfg(target_pointer_width = "64")]
pub mod pmacfg0; // 0x7de
#[cfg(target_pointer_width = "64")]
pub mod pmacfg2; // 0x7df
pub mod prefetch_ctrl; // 0x7d1

mod pmaaddrx; // 0x7e0 ..= 0x7ef
pub use pmaaddrx::*;

pub mod ml2hint; // 0x7f7
pub mod ml2setup; // 0x7f0

pub mod featurectl; // 0xbf9
