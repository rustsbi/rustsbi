//! Timer device ownership, hart-local bindings, and supervisor comparison registers.

mod device;
mod machine;
mod time;

use crate::{
    Error, Result,
    fault_recovery::{csrr, csrw},
    hart::HartLocal,
    irq::{self, LocalIrqDisabled},
};
use alloc::boxed::Box;
use csr::{Read, Write, machine as registers, supervisor};
use machine::{MachineTimer, MachineTimerState};
#[cfg(target_pointer_width = "64")]
use registers::{EnvironmentConfig as Configuration, STCE};
#[cfg(target_pointer_width = "32")]
use registers::{EnvironmentConfigHigh as Configuration, STCEH as STCE};
use spin::Once;

pub use device::TimerDevice;

static TIMER: Once<Timer> = Once::new();

/// Creates and installs the timer service.
pub fn init(device: Option<Box<dyn TimerDevice>>) -> &'static Timer {
    TIMER.call_once(|| Timer::from_device(device))
}

/// Returns the installed timer service.
pub fn get() -> Option<&'static Timer> {
    TIMER.get()
}

enum Comparator {
    Native(&'static MachineTimerState),
    Sstc,
}

/// Timer operations with a fixed notification binding on each initialized hart.
///
/// Native hardware belongs to `MachineTimer`; Sstc uses supervisor comparison CSRs.
/// Operations select the current hart and exclude local interrupt reentry.
/// Deadline operations return [`Error::NotReady`] on an uninitialized hart.
/// Hardware bindings remain reserved for the firmware lifetime.
pub struct Timer {
    machine: Option<MachineTimer>,
    local: HartLocal<Comparator>,
}

impl Timer {
    /// Creates the timer service that owns the optional native device.
    /// Transfer it to Runtime with [`crate::timer::init`].
    /// Runtime initializes each hart after configuring its supervisor environment.
    /// Initialization probes Sstc and enables supervisor comparison when present;
    /// otherwise it selects the native device.
    /// A platform without a native device may use Sstc alone.
    fn from_device(device: Option<Box<dyn TimerDevice>>) -> Self {
        Self {
            machine: device.map(MachineTimer::new),
            local: HartLocal::uninit(),
        }
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
        if Self::enable_sstc()? {
            self.local.try_init_current(Comparator::Sstc)?;
            return self.cancel_with(irq);
        }
        let Some(machine) = self.machine() else {
            // Platforms without native hardware or Sstc do not expose TIME.
            return Ok(());
        };
        let compare = machine.acquire()?;
        self.local.try_init_current(Comparator::Native(compare))?;
        self.cancel_with(irq)
    }

    /// Initial setup owns STCE; M-mode can probe stimecmp before granting S-mode access.
    fn enable_sstc() -> Result<bool> {
        match csrr!(supervisor::TimerCompare) {
            Ok(_) => {}
            Err(Error::IllegalInstruction) => return Ok(false),
            Err(error) => return Err(error),
        }
        #[cfg(target_pointer_width = "32")]
        csrr!(supervisor::TimerCompareHigh)?;

        let current = csrr!(Configuration).map(Configuration::<Read>::from_bits)?;
        if current.read_field::<STCE>() == registers::FeatureState::Enabled {
            return Ok(true);
        }
        let mut value = Configuration::<Write>::from(current);
        value.write_field::<STCE>(registers::FeatureState::Enabled);
        // Initial hart setup excludes IRQs and has no lower-mode context.
        // Preserve translation, firmware features, and other environment fields.
        csrw!(Configuration, usize::from(value))?;
        if csrr!(Configuration)
            .map(Configuration::<Read>::from_bits)?
            .read_field::<STCE>()
            != registers::FeatureState::Enabled
        {
            return Err(Error::NotSupported);
        }
        Ok(true)
    }

    pub(crate) fn on_interrupt(&self, irq: &LocalIrqDisabled) {
        self.machine()
            .expect("MTIP without native timer hardware")
            .on_interrupt(irq);
    }

    fn machine(&self) -> Option<&MachineTimer> {
        self.machine.as_ref()
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
