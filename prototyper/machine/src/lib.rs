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

mod boot;
mod config;
mod console;
mod entry;
mod hart;
pub mod interrupt;
pub mod memory;
mod mmio;
pub mod pmp;
mod pmu;
pub mod power;
mod timer;
mod trap;

#[cfg(test)]
extern crate std;

pub use boot::{BootDtb, BootInfo, NextStage};
pub use console::{Console, ConsoleDevice, ConsoleError};
pub use entry_macros::entry;
pub use hart::{
    HartControl, HartError, HartLocal, HartLocalError, HartLocalGuard, HartState, HartTargets, Ipi,
    IpiError, RemoteFence, RemoteFenceError,
};
pub use mmio::{IoMem, IoMemError, IoMemRegion, IoValue, io_fence};
pub use pmu::{CounterError, CounterInfo, PerformanceCounters};
pub use power::abort;
pub use timer::{Timer, TimerError};
pub use trap::{SbiCall, SbiHandler};

/// The four machine services installed by one coherent interrupt path.
pub struct Interrupts {
    /// Supervisor timer service.
    pub timer: Timer,
    /// Interprocessor notification service.
    pub ipi: Ipi,
    /// Remote instruction and address-translation fence service.
    pub remote_fence: RemoteFence,
    /// Hart lifecycle service.
    pub harts: HartControl,
}
