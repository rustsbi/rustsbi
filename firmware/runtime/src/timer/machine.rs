//! Native timer device ownership, local register state, and MTIP handling.

use super::{Target, TimerDevice};
use crate::{
    Error, Result,
    hart::{HartId, HartLocal},
    irq::LocalIrqDisabled,
};
use alloc::boxed::Box;
use csr::machine;

/// A fixed receiver for firmware-owned machine-timer events.
///
/// Runtime invokes this receiver on the source hart with MIE disabled, after
/// masking and acknowledging the source. No Runtime lock or mutable borrow
/// crosses the callback. Different harts may invoke it concurrently.
/// The receiver must not wait for a later local interrupt.
pub trait TimerHandler: Sync {
    /// Returns the next absolute deadline, or `None` to remain stopped.
    /// The returned decision supersedes deadline changes made inside this callback.
    /// The observed interrupt may be residual after comparator programming.
    fn on_interrupt(&self) -> Option<u64>;
}

pub(super) struct MachineTimer {
    device: Box<dyn TimerDevice>,
    local: HartLocal<MachineTimerState>,
}

pub(super) struct MachineTimerState {
    hart: HartId,
    machine: &'static MachineTimer,
    target: Target,
}

impl MachineTimer {
    pub(super) fn new(device: Box<dyn TimerDevice>) -> Self {
        Self {
            device,
            local: HartLocal::uninit(),
        }
    }

    pub(super) fn acquire(&'static self, target: Target) -> Result<&'static MachineTimerState> {
        let hart = HartId::current()?;
        self.local
            .try_init_current(MachineTimerState {
                hart,
                machine: self,
                target,
            })
            .map_err(|error| match error {
                Error::AlreadyInitialized => Error::Busy,
                other => other,
            })
    }

    pub(super) fn interrupt(&self, irq: &LocalIrqDisabled) {
        self.local.get_current().interrupt(irq);
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
        if matches!(self.target, Target::Supervisor) {
            // SAFETY: This binding owns supervisor delivery on the current hart.
            unsafe { machine::InterruptPending::clear::<machine::STIP>() };
        }
        self.set_enabled(machine::Interrupt::Enabled, irq);
    }

    pub(super) fn cancel(&self, irq: &LocalIrqDisabled) {
        self.set_enabled(machine::Interrupt::Disabled, irq);
        self.machine.device.set_compare(self.hart, u64::MAX);
        if matches!(self.target, Target::Supervisor) {
            // SAFETY: This binding owns supervisor delivery on the current hart.
            unsafe { machine::InterruptPending::clear::<machine::STIP>() };
        }
    }

    fn interrupt(&self, irq: &LocalIrqDisabled) {
        self.set_enabled(machine::Interrupt::Disabled, irq);
        self.machine.device.acknowledge(self.hart);
        match self.target {
            // SAFETY: The current hart's fixed supervisor binding owns STIP.
            Target::Supervisor => unsafe { machine::InterruptPending::set::<machine::STIP>() },
            Target::Machine(handler) => match handler.on_interrupt() {
                Some(deadline) => self.set_deadline(deadline, irq),
                None => self.cancel(irq),
            },
        }
    }

    fn set_enabled(&self, state: machine::Interrupt, _irq: &LocalIrqDisabled) {
        // SAFETY: Timer operations and trap entry keep MIE clear. The native
        // binding is published before enabling this source for its fixed target.
        unsafe {
            machine::InterruptEnable::modify(|_, value| value.write::<machine::MTIE>(state));
        }
    }
}
