#[cfg(feature = "singleton")]
use crate::util::AmoOnceRef;

/// Timer programmer support
pub trait Timer: Send + Sync {
    /// Programs the clock for next event after `stime_value` time.
    ///
    /// `stime_value` is in absolute time. This function must clear the pending timer interrupt bit as well.
    ///
    /// If the supervisor wishes to clear the timer interrupt without scheduling the next timer event,
    /// it can either request a timer interrupt infinitely far into the future (i.e., (uint64_t)-1),
    /// or it can instead mask the timer interrupt by clearing `sie.STIE` CSR bit.
    fn set_timer(&self, stime_value: u64);
}

#[cfg(feature = "singleton")]
static TIMER: AmoOnceRef<dyn Timer> = AmoOnceRef::new();

#[cfg(feature = "singleton")]
/// Init TIMER module
pub fn init_timer(timer: &'static dyn Timer) {
    if !TIMER.try_call_once(timer) {
        panic!("load sbi module when already loaded")
    }
}

#[cfg(feature = "singleton")]
#[inline]
pub(crate) fn probe_timer() -> bool {
    TIMER.get().is_some()
}

#[cfg(feature = "singleton")]
#[inline]
pub(crate) fn set_timer(time_value: u64) -> bool {
    if let Some(timer) = TIMER.get() {
        timer.set_timer(time_value);
        true
    } else {
        false
    }
}
