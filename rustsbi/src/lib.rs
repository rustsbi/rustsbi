//! A minimal RISC-V's SBI implementation in Rust.
//!
//! This library adapts to embedded Rust's `embedded-hal` crate to provide basical SBI features. 
//! When building for own platform, implement traits in this library and pass them to the functions
//! begin with `init`. After that, you may call `rustsbi::ecall` in your own exception handler
//! which would dispatch parameters from supervisor to the traits to execute SBI functions.
//!
//! The library also implements useful functions which may help with platform specific binaries.
//! The `enter_privileged` maybe used to enter the operating system after the initialization 
//! process is finished. The `LOGO` should be printed if necessary when the binary is initializing.
//! 
//! Note that this crate is a library which contains common building blocks in SBI implementation.
//! It is not intended to be used directly; users should build own platforms with this library.
//! RustSBI provides implementations on common platforms in separate platform crates.

#![no_std]
#![feature(asm)]

extern crate alloc;

#[doc(hidden)]
#[macro_use]
pub mod legacy_stdio;
mod ecall;
mod extension;
mod hart_mask;
mod hsm;
mod ipi;
mod logo;
mod privileged;
#[doc(hidden)]
pub mod reset;
mod timer;

const SBI_SPEC_MAJOR: usize = 0;
const SBI_SPEC_MINOR: usize = 2;

// RustSBI implementation ID: 4
// Ref: https://github.com/riscv/riscv-sbi-doc/pull/61
const IMPL_ID_RUSTSBI: usize = 4;
// todo: read from env!("CARGO_PKG_VERSION")
const RUSTSBI_VERSION: usize = 1; 

pub use ecall::handle_ecall as ecall;
pub use ecall::SbiRet;
pub use hart_mask::HartMask;
pub use hsm::Hsm;
pub use ipi::{init_ipi, Ipi};
pub use logo::LOGO;
pub use privileged::enter_privileged;
pub use reset::{init_reset, Reset};
pub use timer::{init_timer, Timer};
