//! Timer capability and its private physical-device role.

use alloc::sync::Arc;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU8, Ordering};

mod arch;
mod device;

pub(crate) use device::TimerDevice;

/// Authority to program the current hart's supervisor timer deadline.
///
/// The physical timer address and register convention remain private. This
/// capability is intentionally not `Clone`.
pub struct Timer {
    device: Arc<dyn TimerDevice>,
}

const DEVICE_EMPTY: u8 = 0;
const DEVICE_WRITING: u8 = 1;
const DEVICE_READY: u8 = 2;

struct InstalledTimer {
    state: AtomicU8,
    device: UnsafeCell<Option<Arc<dyn TimerDevice>>>,
}

// SAFETY: one boot-time writer initializes the Arc before Release
// publication. Readers first observe DEVICE_READY with Acquire, and the
// published Arc is never replaced.
unsafe impl Sync for InstalledTimer {}

static INSTALLED_TIMER: InstalledTimer = InstalledTimer {
    state: AtomicU8::new(DEVICE_EMPTY),
    device: UnsafeCell::new(None),
};

/// Failure while preparing a hart-local timer mechanism.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimerError {
    /// The required architectural timer facility is unavailable.
    Unavailable,
    /// The selected physical hart is outside the validated binding.
    InvalidHart,
}

impl Timer {
    pub(crate) fn new(device: Arc<dyn TimerDevice>) -> Self {
        Self { device }
    }

    /// Programs the current hart's next timer deadline.
    pub fn set_deadline(&self, deadline: u64) {
        self.device.set_compare(arch::current_hart_id(), deadline);
    }

    /// Disables the current hart's timer by selecting the maximal deadline.
    pub fn disable(&self) {
        self.set_deadline(u64::MAX);
    }

    pub(crate) fn trap_device(&self) -> Arc<dyn TimerDevice> {
        Arc::clone(&self.device)
    }
}

pub(crate) fn install(device: Arc<dyn TimerDevice>) -> Result<(), TimerError> {
    INSTALLED_TIMER
        .state
        .compare_exchange(
            DEVICE_EMPTY,
            DEVICE_WRITING,
            Ordering::Acquire,
            Ordering::Relaxed,
        )
        .map_err(|_| TimerError::Unavailable)?;
    // SAFETY: this caller uniquely owns DEVICE_WRITING and readers cannot
    // inspect the slot before the Release store below.
    unsafe { INSTALLED_TIMER.device.get().write(Some(device)) };
    INSTALLED_TIMER.state.store(DEVICE_READY, Ordering::Release);
    Ok(())
}

fn installed() -> Option<&'static dyn TimerDevice> {
    if INSTALLED_TIMER.state.load(Ordering::Acquire) != DEVICE_READY {
        return None;
    }
    // SAFETY: Acquire observed the sole Release publication. The Arc remains
    // stored and immutable for the firmware lifetime.
    unsafe { (&*INSTALLED_TIMER.device.get()).as_deref() }
}

pub(crate) fn prepare_current_hart() -> Result<(), TimerError> {
    installed().map_or(Ok(()), TimerDevice::prepare_current_hart)
}

pub(crate) fn read_time() -> Option<u64> {
    installed().map(TimerDevice::read_time)
}

pub(crate) fn handle_interrupt() -> bool {
    installed().is_some_and(TimerDevice::handle_interrupt)
}
