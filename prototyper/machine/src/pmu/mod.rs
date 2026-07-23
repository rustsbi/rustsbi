//! Typed access to the calling hart's RISC-V performance counters.
//!
//! Counter CSR selection is closed inside this module. Public identifiers are
//! values obtained from one probed capability; they do not carry generic CSR
//! read or write authority.

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU8, Ordering};

use crate::boot::NextMode;

mod control;
mod counters;
mod hart;
mod probe;
mod riscv;

pub use counters::PerformanceCounters;
pub use hart::{CounterError, CounterInfo};

const COUNTERS_EMPTY: u8 = 0;
const COUNTERS_WRITING: u8 = 1;
const COUNTERS_READY: u8 = 2;

struct InstalledCounters {
    state: AtomicU8,
    counters: UnsafeCell<Option<PerformanceCounters>>,
}

// SAFETY: boot publishes one shared handle before Release. The handle remains
// immutable; its own HartLocal storage enforces per-hart mutable access.
unsafe impl Sync for InstalledCounters {}

static INSTALLED_COUNTERS: InstalledCounters = InstalledCounters {
    state: AtomicU8::new(COUNTERS_EMPTY),
    counters: UnsafeCell::new(None),
};

pub(crate) fn install(counters: PerformanceCounters) -> Result<(), CounterError> {
    INSTALLED_COUNTERS
        .state
        .compare_exchange(
            COUNTERS_EMPTY,
            COUNTERS_WRITING,
            Ordering::Acquire,
            Ordering::Relaxed,
        )
        .map_err(|_| CounterError::MechanismFailure)?;
    // SAFETY: this caller uniquely owns COUNTERS_WRITING and readers cannot
    // observe the slot before the Release store.
    unsafe { INSTALLED_COUNTERS.counters.get().write(Some(counters)) };
    INSTALLED_COUNTERS
        .state
        .store(COUNTERS_READY, Ordering::Release);
    Ok(())
}

fn installed() -> Option<&'static PerformanceCounters> {
    if INSTALLED_COUNTERS.state.load(Ordering::Acquire) != COUNTERS_READY {
        return None;
    }
    // SAFETY: Acquire observed the only Release publication, after which the
    // shared handle is immutable for the firmware lifetime.
    unsafe { (&*INSTALLED_COUNTERS.counters.get()).as_ref() }
}

pub(crate) fn prepare_current(mode: NextMode) -> Result<(), CounterError> {
    let Some(counters) = installed() else {
        return Ok(());
    };
    counters.prepare_current()?;
    riscv::prepare_counter_access(mode, counters)
}

#[cfg(test)]
mod tests;
