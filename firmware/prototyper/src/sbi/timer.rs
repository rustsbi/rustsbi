//! SBI supervisor timer.

use runtime::{rustsbi, timer};
use sbi_spec::pmu::firmware_event;

use super::pmu::pmu_firmware_counter_increment;

/// SBI TIME backed by Runtime's timer.
pub struct SbiTimer;

impl SbiTimer {
    pub(crate) fn set_deadline(&self, deadline: u64) {
        if let Some(timer) = timer::get() {
            timer.set_deadline(deadline);
        }
    }
}

impl rustsbi::Timer for SbiTimer {
    fn _rustsbi_probe(&self) -> usize {
        usize::from(timer::get().is_some())
    }

    fn set_timer(&self, deadline: u64) {
        pmu_firmware_counter_increment(firmware_event::SET_TIMER);
        self.set_deadline(deadline);
    }
}
