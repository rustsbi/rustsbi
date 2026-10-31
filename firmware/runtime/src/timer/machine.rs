//! Native timer device ownership and MTIP handling.

use super::TimerDevice;
use crate::{hart::HartId, irq::LocalIrqDisabled};
use alloc::boxed::Box;
use csr::machine;

pub(super) struct MachineTimer {
    device: Box<dyn TimerDevice>,
}

impl MachineTimer {
    pub(super) fn new(device: Box<dyn TimerDevice>) -> Self {
        Self { device }
    }

    pub(super) fn read_time_low(&self) -> Option<usize> {
        self.device.read_time_low()
    }

    #[cfg(target_pointer_width = "32")]
    pub(super) fn read_time_high(&self) -> Option<usize> {
        self.device.read_time_high()
    }

    pub(super) fn read_time(&self) -> Option<u64> {
        self.device.read_time()
    }

    /// Sets the current hart's next machine timer deadline.
    ///
    /// `None` disables the machine timer interrupt.
    pub(super) fn set_next_deadline(
        &self,
        hart: HartId,
        deadline: Option<u64>,
        irq: &LocalIrqDisabled,
    ) {
        self.set_enabled(machine::Interrupt::Disabled, irq);
        self.device.set_compare(hart, deadline.unwrap_or(u64::MAX));
        if deadline.is_some() {
            self.set_enabled(machine::Interrupt::Enabled, irq);
        }
    }

    /// Acknowledges the current hart's machine timer interrupt.
    pub(super) fn acknowledge_interrupt(&self, hart: HartId, irq: &LocalIrqDisabled) {
        self.set_enabled(machine::Interrupt::Disabled, irq);
        self.device.acknowledge(hart);
    }

    fn set_enabled(&self, state: machine::Interrupt, _irq: &LocalIrqDisabled) {
        // SAFETY:
        // 1. Timer operations and trap entry keep MIE clear.
        // 2. The current hart's timer state is published before enabling this source.
        unsafe {
            machine::InterruptEnable::modify(|_, value| value.set::<machine::MTIE>(state));
        }
    }
}
