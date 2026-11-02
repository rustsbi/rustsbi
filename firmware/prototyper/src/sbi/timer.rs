//! SBI supervisor timer.

use alloc::boxed::Box;
use runtime::{
    Result, rustsbi,
    timer::{Timer, TimerDevice},
};
use sbi_spec::pmu::firmware_event;

use super::pmu::pmu_firmware_counter_increment;

/// SBI TIME backed by Sstc or a platform machine timer.
pub struct SbiTimer {
    timer: Option<Timer>,
    device: Option<&'static dyn TimerDevice>,
}

impl SbiTimer {
    pub(crate) fn new(device: Option<Box<dyn TimerDevice>>) -> Result<Self> {
        let timer = Timer::detect()?;
        // The policy and the platform time source both live for the firmware
        // lifetime, so the device can be published once from boot.
        let device = device.map(|device| &*Box::leak(device));
        Ok(Self { timer, device })
    }

    /// Returns the platform timer device used as the emulated time source.
    pub(crate) fn device(&self) -> Option<&'static dyn TimerDevice> {
        self.device
    }

    pub(crate) fn set_deadline(&self, deadline: u64) {
        if let Some(timer) = &self.timer {
            timer.set_deadline(deadline);
        } else if let Some(device) = &self.device {
            device.set_deadline(deadline);
        }
    }
}

impl rustsbi::Timer for SbiTimer {
    fn _rustsbi_probe(&self) -> usize {
        usize::from(self.timer.is_some() || self.device.is_some())
    }

    fn set_timer(&self, deadline: u64) {
        pmu_firmware_counter_increment(firmware_event::SET_TIMER);
        self.set_deadline(deadline);
    }
}
