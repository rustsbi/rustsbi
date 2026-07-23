//! Machine-mode mechanisms for RustSBI Prototyper.
//!
//! This crate is the soundness boundary between safe firmware policy and raw
//! RISC-V machine state. During the migration, machine-mode mechanisms move
//! here behind capability-oriented APIs; protocol policy remains in the
//! `rustsbi-prototyper` executable.

#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]

extern crate alloc;

pub mod aia;
mod boot;
pub mod clint;
#[path = "startup/config.rs"]
mod config;
mod console;
pub mod drivers;
mod hart;
pub mod memory;
pub mod pmp;
mod pmu;
mod power;
mod startup;
#[cfg(feature = "mtest")]
mod test_support;
mod timer;
mod trap;

#[cfg(test)]
extern crate std;

pub use boot::{BootDtb, BootInfo, NextStage};
pub use console::{Console, ConsoleError};
pub use hart::{
    HartControl, HartError, HartLocal, HartLocalError, HartLocalGuard, HartStatus, HartTargets,
    Ipi, IpiError, RemoteFence, RemoteFenceError,
};
pub use machine_macros::{entry, mtest};
pub use pmu::{CounterError, CounterInfo, PerformanceCounters};
pub use power::abort;
#[cfg(feature = "mtest")]
pub use test_support::{MachineTests as Tests, prepare as prepare_tests};

#[cfg(feature = "mtest")]
#[doc(hidden)]
pub mod __private_mtest {
    pub use machine_test::Descriptor;
}
pub use power::{Power, PowerError, PowerReason, RebootKind};
pub use timer::{Timer, TimerError};
pub use trap::{SbiCall, SbiHandler, SbiResponse, TrapEvent};
