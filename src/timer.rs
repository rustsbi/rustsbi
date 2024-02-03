/// Timer programmer support extension.
pub trait Timer {
    /// Programs the clock for the next event after `stime_value` time.
    ///
    /// `stime_value` is in absolute time. This function must clear the pending timer interrupt bit as well.
    ///
    /// If the supervisor wishes to clear the timer interrupt without scheduling the next timer event,
    /// it can either request a timer interrupt infinitely far into the future (i.e., (uint64_t)-1),
    /// or it can instead mask the timer interrupt by clearing `sie.STIE` CSR bit.
    fn set_timer(&self, stime_value: u64);
}

impl<T: Timer> Timer for &T {
    #[inline]
    fn set_timer(&self, stime_value: u64) {
        T::set_timer(self, stime_value)
    }
}
