//! Native timer device ownership, local register state, and MTIP handling.

use super::TimerDevice;
use crate::{
    Error, Result,
    hart::{HartId, HartLocal},
    irq::LocalIrqDisabled,
};
use alloc::boxed::Box;
use csr::machine;

pub(super) struct MachineTimer {
    device: Box<dyn TimerDevice>,
    local: HartLocal<MachineTimerState>,
}

pub(super) struct MachineTimerState {
    hart: HartId,
    machine: &'static MachineTimer,
}

impl MachineTimer {
    pub(super) fn new(device: Box<dyn TimerDevice>) -> Self {
        Self {
            device,
            local: HartLocal::uninit(),
        }
    }

    pub(super) fn acquire(&'static self) -> Result<&'static MachineTimerState> {
        let hart = HartId::current()?;
        self.local
            .try_init_current(MachineTimerState { hart, machine: self })
            .map_err(|error| match error {
                Error::AlreadyInitialized => Error::Busy,
                other => other,
            })
    }

    pub(super) fn on_interrupt(&self, irq: &LocalIrqDisabled) {
        self.local.get_current().on_interrupt(irq);
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
}

impl MachineTimerState {
    pub(super) fn set_deadline(&self, deadline: u64, irq: &LocalIrqDisabled) {
        self.set_enabled(machine::Interrupt::Disabled, irq);
        self.machine.device.set_compare(self.hart, deadline);
        // SAFETY: This binding owns supervisor delivery on the current hart.
        unsafe { machine::InterruptPending::clear_bit::<machine::STIP>() };
        self.set_enabled(machine::Interrupt::Enabled, irq);
    }

    pub(super) fn cancel(&self, irq: &LocalIrqDisabled) {
        self.set_enabled(machine::Interrupt::Disabled, irq);
        self.machine.device.set_compare(self.hart, u64::MAX);
        // SAFETY: This binding owns supervisor delivery on the current hart.
        unsafe { machine::InterruptPending::clear_bit::<machine::STIP>() };
    }

    fn on_interrupt(&self, irq: &LocalIrqDisabled) {
        self.set_enabled(machine::Interrupt::Disabled, irq);
        self.machine.device.acknowledge(self.hart);
        // SAFETY: The current hart's fixed supervisor binding owns STIP.
        unsafe { machine::InterruptPending::set_bit::<machine::STIP>() };
    }

    fn set_enabled(&self, state: machine::Interrupt, _irq: &LocalIrqDisabled) {
        // SAFETY: Timer operations and trap entry keep MIE clear. The native
        // binding is published before enabling this source for its fixed target.
        unsafe {
            machine::InterruptEnable::modify(|_, value| value.write_field::<machine::MTIE>(state));
        }
    }
}
