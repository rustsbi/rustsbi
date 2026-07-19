//! Typed access to the calling hart's architectural performance counters.
//!
//! Counter CSR selection is closed inside this module. Public identifiers are
//! values obtained from one probed capability; they do not carry generic CSR
//! read or write authority.

mod arch;
mod control;
mod counters;
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
mod probe;
mod state;

pub use counters::PerformanceCounters;
pub use state::{CounterError, CounterId, CounterInfo};

#[cfg(test)]
mod tests;
