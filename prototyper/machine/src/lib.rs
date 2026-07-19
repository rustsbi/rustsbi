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

#[cfg(all(feature = "jump", feature = "payload"))]
compile_error!("jump and payload select different next-stage providers");

#[cfg(all(feature = "mtest", any(feature = "jump", feature = "payload")))]
compile_error!("mtest owns the post-initialization action and cannot hand off a bundled stage");

mod boot;
mod config;
mod console;
mod counter;
mod csr;
pub mod drivers;
mod hart;
mod interrupt;
mod memory;
mod pmp;
mod power;
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
mod startup;
#[cfg(all(
    feature = "mtest",
    any(target_arch = "riscv32", target_arch = "riscv64")
))]
mod test_support;
mod timer;
mod trap;

#[cfg(test)]
extern crate std;

pub use boot::{BootDtb, BootInfo, NextStage, enter_next_stage};
pub use console::{Console, ConsoleError};
pub use counter::{CounterError, CounterId, CounterInfo, PerformanceCounters};
pub use hart::{
    HartControl, HartError, HartLocal, HartLocalError, HartLocalGuard, HartStatus, HartTargets,
    Ipi, IpiError, RemoteFence, RemoteFenceError,
};
pub use machine_macros::{entry, mtest};
pub use memory::{SModeMemory, SModeMemoryError, SModeReader, SModeWriter};
pub use power::abort;
#[cfg(all(
    feature = "mtest",
    any(target_arch = "riscv32", target_arch = "riscv64")
))]
pub use test_support::{MachineTests as Tests, prepare as prepare_tests};

#[cfg(feature = "mtest")]
#[doc(hidden)]
pub mod __private_mtest {
    pub use machine_test::Descriptor;
}
pub use power::{Power, PowerError, PowerReason, RebootKind};
pub use timer::{Timer, TimerError};
pub use trap::{Cause, Trap, TrapHandler};

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
#[doc(hidden)]
pub use startup::raw_entry as __private_entry;
