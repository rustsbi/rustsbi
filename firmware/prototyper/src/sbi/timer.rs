//! Timer programming.
//!
//! # References
//!
//! - Specification: [RISC-V SBI TIME extension](https://docs.riscv.org/reference/sbi/v3.0/ext-time.html) —
//!   absolute deadlines and timer-interrupt behavior.

use super::pmu::pmu_firmware_counter_increment;
use sbi_spec::pmu::firmware_event;

/// SBI TIME extension using Runtime's current-hart timer service.
pub struct SbiTimer;

impl runtime::rustsbi::Timer for SbiTimer {
    /// Sets the timer for the current hart.
    #[inline]
    fn set_timer(&self, stime_value: u64) {
        pmu_firmware_counter_increment(firmware_event::SET_TIMER);
        runtime::timer::Timer::current()
            .and_then(|timer| timer.set_deadline(stime_value))
            .expect("BUG: published SBI TIME has no initialized current-hart timer");
    }
}
