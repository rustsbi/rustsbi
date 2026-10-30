//! Timer device ownership, hart-local bindings, and supervisor comparison registers.

mod device;
mod machine;
mod time;

use crate::{
    Error, Result,
    csr::csrr,
    hart::HartLocal,
    irq::{self, LocalIrqDisabled},
};
use alloc::boxed::Box;
use csr::{machine as registers, supervisor};
use machine::{MachineTimer, MachineTimerState};
use spin::Once;

pub use device::TimerDevice;
pub use machine::TimerHandler;

#[derive(Clone, Copy)]
enum Target {
    Supervisor,
    Machine(&'static dyn TimerHandler),
}

static FIRMWARE: Once<Timer> = Once::new();

enum Comparator {
    Native(&'static MachineTimerState),
    Sstc,
}

enum NativeTimer {
    Owned(MachineTimer),
    Borrowed(&'static MachineTimer),
}

/// Timer operations with a fixed notification binding on each initialized hart.
///
/// Native hardware belongs to `MachineTimer`; Sstc uses supervisor comparison CSRs.
/// Operations select the current hart and exclude local interrupt reentry.
/// Deadline operations return [`Error::NotReady`] on an uninitialized hart.
/// Hardware bindings remain reserved for the firmware lifetime.
pub struct Timer {
    machine: Option<NativeTimer>,
    local: HartLocal<Comparator>,
}

impl Timer {
    /// Creates a supervisor timer that owns the optional native device.
    /// Move it into permanent storage before registering it with Runtime.
    /// Runtime initializes each hart after configuring its supervisor environment.
    /// A platform without a native device may use Sstc alone.
    pub fn new(device: Option<Box<dyn TimerDevice>>) -> Self {
        Self {
            machine: device.map(|device| NativeTimer::Owned(MachineTimer::new(device))),
            local: HartLocal::uninit(),
        }
    }

    /// Acquires this hart's native comparator for a fixed firmware receiver.
    /// Returns [`Error::Busy`] if the comparator already has an owner.
    pub fn acquire(handler: &'static dyn TimerHandler) -> Result<&'static Self> {
        irq::with_local_disabled(|irq| {
            if !crate::boot::traps_ready() {
                return Err(Error::NotReady);
            }
            let supervisor = crate::boot::timer().ok_or(Error::NotReady)?;
            let machine = supervisor.machine().ok_or(Error::NotSupported)?;
            let timer = FIRMWARE.call_once(|| Self {
                machine: Some(NativeTimer::Borrowed(machine)),
                local: HartLocal::uninit(),
            });
            if timer.local.try_get_current().is_some() {
                return Err(Error::Busy);
            }
            let compare = machine.acquire(Target::Machine(handler))?;
            timer
                .local
                .try_init_current(Comparator::Native(compare))
                .map_err(|error| match error {
                    Error::AlreadyInitialized => Error::Busy,
                    other => other,
                })?;
            // Publish native state before programming the device.
            timer.cancel_with(irq)?;
            Ok(timer)
        })
    }

    /// Returns whether this hart has an initialized timer binding.
    pub fn is_available(&self) -> bool {
        self.local.try_get_current().is_some()
    }

    /// Replaces the current hart's absolute deadline and rearms its timer.
    pub fn set_deadline(&self, deadline: u64) -> Result<()> {
        irq::with_local_disabled(|irq| {
            let compare = self.local.try_get_current().ok_or(Error::NotReady)?;
            match compare {
                Comparator::Native(timer) => timer.set_deadline(deadline, irq),
                Comparator::Sstc => set_supervisor_deadline(deadline, irq),
            }
            Ok(())
        })
    }

    /// Cancels this hart's deadline and retracts pending state maintained by it.
    /// Cancellation keeps the hardware and notification binding reserved.
    pub fn cancel(&self) -> Result<()> {
        irq::with_local_disabled(|irq| self.cancel_with(irq))
    }

    fn cancel_with(&self, irq: &LocalIrqDisabled) -> Result<()> {
        let compare = self.local.try_get_current().ok_or(Error::NotReady)?;
        match compare {
            Comparator::Native(timer) => timer.cancel(irq),
            Comparator::Sstc => set_supervisor_deadline(u64::MAX, irq),
        }
        Ok(())
    }

    pub(crate) fn init_current(&'static self, irq: &LocalIrqDisabled) -> Result<()> {
        if !crate::boot::traps_ready() {
            return Err(Error::NotReady);
        }
        if self.local.try_get_current().is_some() {
            return Ok(());
        }
        if Self::sstc_available()? {
            self.local.try_init_current(Comparator::Sstc)?;
            return self.cancel_with(irq);
        }
        let Some(machine) = self.machine() else {
            // Platforms without native hardware or Sstc do not expose TIME.
            return Ok(());
        };
        let compare = machine.acquire(Target::Supervisor)?;
        self.local.try_init_current(Comparator::Native(compare))?;
        self.cancel_with(irq)
    }

    /// Detects supervisor comparison access after Runtime trap installation.
    pub fn supports_sstc() -> bool {
        crate::boot::traps_ready() && csrr!(supervisor::TimerCompare).is_ok()
    }

    fn sstc_available() -> Result<bool> {
        #[cfg(target_pointer_width = "64")]
        type Configuration = registers::EnvironmentConfiguration;
        #[cfg(target_pointer_width = "32")]
        type Configuration = registers::EnvironmentConfigurationHigh;
        let enabled = csrr!(Configuration).is_ok_and(|config| {
            #[cfg(target_pointer_width = "64")]
            let config = registers::EnvironmentConfigurationReadValue::from_bits(config);
            #[cfg(target_pointer_width = "32")]
            let config = registers::EnvironmentConfigurationHighReadValue::from_bits(config);
            config.supervisor_timer_enabled()
        });
        if !enabled {
            return Ok(false);
        }
        csrr!(supervisor::TimerCompare).map_err(|_| Error::NotSupported)?;
        #[cfg(target_pointer_width = "32")]
        csrr!(supervisor::TimerCompareHigh).map_err(|_| Error::NotSupported)?;
        Ok(true)
    }

    pub(crate) fn on_interrupt(&self, irq: &LocalIrqDisabled) {
        self.machine()
            .expect("MTIP without native timer hardware")
            .on_interrupt(irq);
    }

    fn machine(&self) -> Option<&MachineTimer> {
        self.machine.as_ref().map(|timer| match timer {
            NativeTimer::Owned(machine) => machine,
            NativeTimer::Borrowed(machine) => *machine,
        })
    }
}

fn set_supervisor_deadline(deadline: u64, _irq: &LocalIrqDisabled) {
    // SAFETY: Per-hart initialization confirmed Sstc and STCE. The caller
    // excludes interrupt reentry throughout the comparison update.
    unsafe {
        #[cfg(target_pointer_width = "64")]
        supervisor::TimerCompare::write((deadline as usize).into());
        #[cfg(target_pointer_width = "32")]
        {
            supervisor::TimerCompare::write(usize::MAX.into());
            supervisor::TimerCompareHigh::write(((deadline >> 32) as usize).into());
            supervisor::TimerCompare::write((deadline as usize).into());
        }
    }
}
