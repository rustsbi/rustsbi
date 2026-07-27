//! Publication and per-hart installation of the machine PMP policy.

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::ops::Range;
use core::sync::atomic::{AtomicU32, Ordering};

use super::hardware;
use super::state::*;

pub(super) fn machine_image() -> Result<Region, PmpError> {
    unsafe extern "C" {
        static sbi_start: u8;
        static sbi_end: u8;
    }

    let start = core::ptr::addr_of!(sbi_start) as usize;
    let end = core::ptr::addr_of!(sbi_end) as usize;
    Region::new(start, end)
}

pub(crate) fn machine_image_range() -> Option<Range<usize>> {
    machine_image().ok().map(|region| region.start..region.end)
}

const POLICY_EMPTY: u32 = 0;
const POLICY_WRITING: u32 = 1;
const POLICY_READY: u32 = 2;

struct PublishedPolicy {
    state: AtomicU32,
    ranges: UnsafeCell<Option<&'static [Range<usize>]>>,
    configuration: UnsafeCell<Option<&'static Configuration>>,
}

// SAFETY: one claimant initializes the leaked immutable slice before Release
// publication; every reader first observes the ready state with Acquire.
unsafe impl Sync for PublishedPolicy {}

static POLICY: PublishedPolicy = PublishedPolicy {
    state: AtomicU32::new(POLICY_EMPTY),
    ranges: UnsafeCell::new(None),
    configuration: UnsafeCell::new(None),
};

pub(crate) fn publish(
    ranges: &[Range<usize>],
    configuration: &Configuration,
) -> Result<(), PmpError> {
    let ranges = ranges
        .iter()
        .map(|range| Region::new(range.start, range.end))
        .collect::<Result<Vec<_>, _>>()?;
    let ranges = ranges
        .into_iter()
        .map(|region| region.start..region.end)
        .collect::<Vec<_>>();
    POLICY
        .state
        .compare_exchange(
            POLICY_EMPTY,
            POLICY_WRITING,
            Ordering::Acquire,
            Ordering::Relaxed,
        )
        .map_err(|_| PmpError::InconsistentCapability)?;
    let ranges = Box::leak(ranges.into_boxed_slice());
    let configuration = Box::leak(Box::new(configuration.clone()));
    // SAFETY: this caller owns the writing state and the leaked slice is
    // immutable for the remaining firmware lifetime.
    unsafe { POLICY.ranges.get().write(Some(ranges)) };
    // SAFETY: the same one-time claimant owns publication of this immutable
    // semantic configuration.
    unsafe { POLICY.configuration.get().write(Some(configuration)) };
    POLICY.state.store(POLICY_READY, Ordering::Release);
    Ok(())
}

pub(crate) fn configure_current_hart() -> Result<(), PmpError> {
    if POLICY.state.load(Ordering::Acquire) != POLICY_READY {
        return Err(PmpError::InconsistentCapability);
    }
    // SAFETY: Acquire observed the one-time immutable publication.
    let ranges = unsafe { (&*POLICY.ranges.get()).ok_or(PmpError::InconsistentCapability)? };
    // SAFETY: the same Acquire observation publishes both immutable fields.
    let configuration =
        unsafe { (&*POLICY.configuration.get()).ok_or(PmpError::InconsistentCapability)? };
    let ranges = ranges
        .iter()
        .map(|range| Region::new(range.start, range.end))
        .collect::<Result<Vec<_>, _>>()?;
    hardware::configure_current_hart(&ranges, configuration, crate::config::TRUSTED_TARGET)
}
