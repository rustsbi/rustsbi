//! Timer programming.
//!
//! # References
//!
//! - Specification: [RISC-V SBI TIME extension](https://docs.riscv.org/reference/sbi/v3.0/ext-time.html) —
//!   absolute deadlines and timer-interrupt behavior.

#![forbid(unsafe_code)]

use super::pmu::pmu_firmware_counter_increment;
use crate::driver::TimerDevice;
use crate::riscv::csr::{mie, mip, stimecmp};
use crate::riscv::current_hartid;
use crate::sbi::features::{Extension, hart_has_extension};
use alloc::boxed::Box;
use sbi_spec::pmu::firmware_event;
use spin::Mutex;

/// SBI timer extension.
pub struct SbiTimer {
    /// Timer device: CLINT `mtimecmp` registers or the Sstc `stimecmp` CSR.
    device: Mutex<Box<dyn TimerDevice>>,
}

impl rustsbi::Timer for SbiTimer {
    /// Sets the timer for the current hart.
    #[inline]
    fn set_timer(&self, stime_value: u64) {
        pmu_firmware_counter_increment(firmware_event::SET_TIMER);
        let hart_id = current_hartid();

        if hart_has_extension(hart_id, Extension::Sstc) {
            stimecmp::set(stime_value);
        } else {
            self.set_timer_for_hart(hart_id, stime_value);
            mip::clear_stimer();
            mie::set_mtimer();
        }
    }
}

impl SbiTimer {
    /// Creates a new SBI timer extension.
    #[inline]
    pub(crate) fn new(device: Box<dyn TimerDevice>) -> Self {
        Self {
            device: Mutex::new(device),
        }
    }

    /// Gets the lower 32 bits of machine time.
    #[inline]
    pub fn get_time(&self) -> usize {
        self.device.lock().read_time() as usize
    }

    /// Gets the upper 32 bits of machine time.
    #[inline]
    pub fn get_timeh(&self) -> usize {
        (self.device.lock().read_time() >> 32) as usize
    }

    /// Programs a hart's timer comparison value.
    #[inline]
    fn set_timer_for_hart(&self, hart_id: usize, value: u64) {
        self.device.lock().set_timer(hart_id, value);
    }

    /// Cancels the current hart's pending timer interrupt.
    #[inline]
    pub(crate) fn clear(&self) {
        self.set_timer_for_hart(current_hartid(), u64::MAX);
    }
}

/// Cancels the current hart's pending timer interrupt.
#[inline]
pub fn clear() {
    match crate::sbi::timer() {
        Some(timer) => timer.clear(),
        None => error!("SBI timer device not initialized"),
    }
}
