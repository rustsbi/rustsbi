//! Timer programming.
//!
//! # References
//!
//! - Specification: [RISC-V SBI TIME extension](https://docs.riscv.org/reference/sbi/v3.0/ext-time.html) —
//!   absolute deadlines and timer-interrupt behavior.

#![forbid(unsafe_code)]

use super::pmu::pmu_firmware_counter_increment;
use crate::driver::timer::TimerDevice;
use crate::riscv::csr::{mie, mip, stimecmp};
use crate::sbi::features::{Extension, hart_has_extension};
use runtime::hart::HartId;
use sbi_spec::pmu::firmware_event;

/// SBI TIME extension using a published platform timer.
pub struct SbiTimer {
    device: &'static TimerDevice,
}

impl runtime::rustsbi::Timer for SbiTimer {
    /// Sets the timer for the current hart.
    #[inline]
    fn set_timer(&self, stime_value: u64) {
        pmu_firmware_counter_increment(firmware_event::SET_TIMER);
        let hart_id = HartId::current()
            .expect("BUG: current hart exceeds Runtime capacity")
            .as_usize();

        if hart_has_extension(hart_id, Extension::Sstc) {
            stimecmp::set(stime_value);
        } else {
            self.device.set_timer(hart_id, stime_value);
            mip::clear_stimer();
            mie::set_mtimer();
        }
    }
}

impl SbiTimer {
    /// Adapts the selected timer to the SBI TIME extension.
    pub(crate) fn new(device: &'static TimerDevice) -> Self {
        Self { device }
    }
}
