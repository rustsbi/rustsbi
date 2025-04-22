//! RISC-V Supervisor Binary Interface test suite

#![no_std]
#![deny(warnings, missing_docs)]

mod thread;

pub extern crate sbi_rt as sbi;

#[cfg(feature = "log")]
mod log_test;

#[cfg(feature = "log")]
pub use log_test::Testing;

// §4
mod base;
pub use base::{Case as BaseCase, Extensions, test as test_base};
// §6
mod time;
pub use time::{Case as TimerCase, test as test_timer};
// §7
mod spi;
pub use spi::{Case as IpiCase, test as test_ipi};
// §8
// pub mod rfnc;
// §9
mod hsm;
pub use hsm::{Case as HsmCase, test as test_hsm};
// §10
// pub mod srst;
// §11
// pub mod pmu;
// §12
mod dbcn;
pub use dbcn::{Case as DbcnCase, test as test_dbcn};
