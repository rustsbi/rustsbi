//! SBI TIME policy over Runtime's supervisor timer.

#![forbid(unsafe_code)]

use super::pmu::pmu_firmware_counter_increment;
use runtime::{rustsbi, timer::Timer};
use sbi_spec::pmu::firmware_event;

/// SBI TIME extension with permission to program the supervisor timer.
pub struct SbiTimer {
    timer: Timer,
}

impl rustsbi::Timer for SbiTimer {
    fn _rustsbi_probe(&self) -> usize {
        usize::from(self.timer.is_available())
    }

    fn set_timer(&self, deadline: u64) {
        pmu_firmware_counter_increment(firmware_event::SET_TIMER);
        self.timer
            .set_deadline(deadline)
            .expect("supervisor timer not initialized on this hart");
    }
}

impl SbiTimer {
    pub(crate) const fn new(timer: Timer) -> Self {
        Self { timer }
    }

    pub(crate) fn timer(&self) -> &Timer {
        &self.timer
    }
}
