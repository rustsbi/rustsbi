/// Timer programmer support
pub trait Timer: Send {
    /// Programs the clock for next event after `stime_value` time.
    ///
    /// `stime_value` is in absolute time. This function must clear the pending timer interrupt bit as well.
    ///
    /// If the supervisor wishes to clear the timer interrupt without scheduling the next timer event,
    /// it can either request a timer interrupt infinitely far into the future (i.e., (uint64_t)-1),
    /// or it can instead mask the timer interrupt by clearing sie.STIE.
    fn set_timer(&mut self, stime_value: u64);
}

use alloc::boxed::Box;
use spin::Mutex;

lazy_static::lazy_static! {
    static ref TIMER: Mutex<Option<Box<dyn Timer>>> = Mutex::new(None);
}

#[doc(hidden)] // use through a macro
pub fn init_timer<T: Timer + Send + 'static>(ipi: T) {
    *TIMER.lock() = Some(Box::new(ipi));
}

#[inline]
pub(crate) fn probe_timer() -> bool {
    TIMER.lock().as_ref().is_some()
}

#[inline]
pub(crate) fn set_timer(time_value: u64) -> bool {
    if let Some(timer) = TIMER.lock().as_mut() {
        timer.set_timer(time_value);
        true
    } else {
        false
    }
}
