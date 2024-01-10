//! RISC-V Supervisor Binary Interface test suite

#![no_std]
#![deny(warnings, missing_docs)]
#![feature(naked_functions, asm_const)]

mod thread;

pub extern crate sbi_rt as sbi;

#[cfg(feature = "log")]
pub extern crate log_crate as log;

#[cfg(feature = "log")]
mod log_test;

#[cfg(feature = "log")]
pub use log_test::Testing;

// §4
mod base;
pub use base::{test as test_base, Case as BaseCase, Extensions};
// §6
mod time;
pub use time::{test as test_timer, Case as TimerCase};
// §7
mod spi;
pub use spi::{test as test_ipi, Case as IpiCase};
// §8
// pub mod rfnc;
// §9
mod hsm;
pub use hsm::{test as test_hsm, Case as HsmCase};
// §10
// pub mod srst;
// §11
// pub mod pmu;
// §12
mod dbcn;
pub use dbcn::{test as test_dbcn, Case as DbcnCase};
