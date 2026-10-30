//! SBI TIME policy over Runtime's supervisor timer.

#![forbid(unsafe_code)]

use super::pmu::{FirmwareEvent, SbiPmu};
use runtime::{rustsbi, timer::Timer};

/// SBI TIME extension with permission to program the supervisor timer.
pub struct SbiTimer {
    timer: &'static Timer,
}

impl rustsbi::Timer for SbiTimer {
    fn _rustsbi_probe(&self) -> usize {
        usize::from(self.timer.is_available())
    }

    fn set_timer(&self, deadline: u64) {
        SbiPmu::record_event(FirmwareEvent::SetTimer);
        self.timer
            .set_deadline(deadline)
            .expect("supervisor timer not initialized on this hart");
    }
}

impl SbiTimer {
    pub(crate) const fn new(timer: &'static Timer) -> Self {
        Self { timer }
    }
}
