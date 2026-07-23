//! Current-hart supervisor timer deadlines.

use core::ptr;
use core::sync::atomic::{AtomicPtr, Ordering};

mod riscv;
pub(crate) mod sstc;

/// Authority to program the current hart's supervisor timer deadline.
///
/// The physical timer address and register convention remain private. This
/// capability is intentionally not `Clone`.
pub struct Timer {
    operations: &'static Operations,
}

/// Closed operations installed by one validated timer mechanism.
///
/// This is a private function table rather than a device abstraction: Sstc is
/// an architectural CSR facility, while CLINT is sensitive MMIO. The two
/// mechanisms share only the timer semantics consumed by trap and SBI paths.
pub(crate) struct Operations {
    pub(crate) prepare_current_hart: fn() -> Result<(), TimerError>,
    pub(crate) read_time: fn() -> u64,
    pub(crate) set_deadline: fn(u64),
    pub(crate) handle_interrupt: fn() -> bool,
}

static INSTALLED_TIMER: AtomicPtr<Operations> = AtomicPtr::new(ptr::null_mut());

/// Failure while preparing a hart-local timer mechanism.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimerError {
    /// The required architectural timer facility is unavailable.
    Unavailable,
    /// The selected physical hart is outside the validated binding.
    InvalidHart,
}

impl Timer {
    pub(crate) const fn new(operations: &'static Operations) -> Self {
        Self { operations }
    }

    /// Programs the current hart's next timer deadline.
    pub fn set_deadline(&self, deadline: u64) {
        (self.operations.set_deadline)(deadline);
    }

    /// Disables the current hart's timer by selecting the maximal deadline.
    pub fn disable(&self) {
        self.set_deadline(u64::MAX);
    }

    pub(crate) const fn operations(&self) -> &'static Operations {
        self.operations
    }
}

pub(crate) fn install(operations: &'static Operations) -> Result<(), TimerError> {
    let operations = ptr::from_ref(operations).cast_mut();
    INSTALLED_TIMER
        .compare_exchange(
            ptr::null_mut(),
            operations,
            Ordering::AcqRel,
            Ordering::Relaxed,
        )
        .map_err(|_| TimerError::Unavailable)?;
    Ok(())
}

fn installed() -> Option<&'static Operations> {
    let operations = INSTALLED_TIMER.load(Ordering::Acquire);
    // SAFETY: every non-null value came from a `&'static Operations` in the
    // sole successful installation and is never replaced or freed.
    unsafe { operations.as_ref() }
}

pub(crate) fn prepare_current_hart() -> Result<(), TimerError> {
    installed().map_or(Ok(()), |operations| (operations.prepare_current_hart)())
}

pub(crate) fn read_time() -> Option<u64> {
    installed().map(|operations| (operations.read_time)())
}

pub(crate) fn handle_interrupt() -> bool {
    installed().is_some_and(|operations| (operations.handle_interrupt)())
}

#[crate::mtest]
fn installed_timer_has_current_hart_deadline_semantics() {
    prepare_current_hart().expect("installed timer must prepare the admitted hart");
    let before = read_time().expect("installed timer must expose architectural time");
    let after = read_time().expect("installed timer must remain readable");
    assert!(after >= before);
    let operations = installed().expect("timer installation precedes M-test execution");
    (operations.set_deadline)(u64::MAX);
}
