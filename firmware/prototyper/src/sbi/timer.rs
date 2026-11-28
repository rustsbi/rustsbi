//! SBI supervisor timer.

use super::pmu::{firmware_event_code, pmu_firmware_counter_increment};
use runtime::{Error, rustsbi, timer};

/// SBI TIME backed by Runtime's timer.
pub struct SbiTimer;

impl rustsbi::Timer for SbiTimer {
    fn _rustsbi_probe(&self) -> usize {
        match timer::get() {
            Ok(timer) => usize::from(timer.is_some()),
            Err(error) => {
                error!("SBI timer probe failed: {error}");
                crate::fail::stop();
            }
        }
    }

    fn set_timer(&self, deadline: u64) {
        pmu_firmware_counter_increment(firmware_event_code!(SET_TIMER));
        if let Err(error) =
            timer::get().and_then(|timer| timer.ok_or(Error::NotSupported)?.set_deadline(deadline))
        {
            error!("SBI timer request failed: {error}");
            crate::fail::stop();
        }
    }
}
