//! Hardware discovery and the scoped current-hart access contract.

use super::{Counter, Counters, Pmu, hardware};
use crate::{Error, Result, fault_recovery::csrr, irq};
use csr::values::counter::InhibitedCounters;
use spin::Mutex;

impl Pmu {
    pub(crate) fn init_current(&self, inhibit: bool) -> Result<()> {
        if self.local.try_get_current().is_some() {
            return Err(Error::AlreadyInitialized);
        }
        let _hardware = self.hardware.lock();
        let widths = hardware::discover()?;
        self.local
            .try_init_current(Mutex::new(Counter { widths, inhibit }))?;
        Ok(())
    }

    pub(crate) fn reset_current(&self) -> Result<()> {
        self.with_current(|counters| {
            counters.reset_programmable();
            Ok(())
        })
    }

    /// Runs one operation with current-hart counter access and local IRQs excluded.
    /// Returns `NotReady` before initialization or outside Runtime's machine entry.
    /// Returns `Busy` on recursive access before acquiring the shared hardware lock.
    /// The view cannot escape the closure or be sent to another hart.
    /// The closure must not wait for local IRQs or locks held by an interrupted caller.
    /// Hardware transactions serialize across harts; physically shared counters remain shared.
    pub fn with_current<T>(&self, operation: impl FnOnce(&Counters<'_>) -> Result<T>) -> Result<T> {
        let state = self.local.try_get_current().ok_or(Error::NotReady)?;
        if !crate::boot::traps_ready() || usize::from(csr::machine::Scratch::read()) != 0 {
            return Err(Error::NotReady);
        }
        irq::with_local_disabled(|irq| {
            let state = state.try_lock().ok_or(Error::Busy)?;
            let _hardware = self.hardware.lock();
            operation(&Counters {
                state: &state,
                _irq: irq,
            })
        })
    }
}

/// Applies initial counter defaults even when no PMU extension is selected.
pub(crate) fn configure_initial() -> Result<bool> {
    let _hardware = crate::boot::pmu().map(|pmu| pmu.hardware.lock());
    match csrr!(csr::machine::CounterInhibit) {
        Ok(_) => {
            // SAFETY: Boot owns the initial counters and excludes local IRQs.
            unsafe { csr::machine::CounterInhibit::write(InhibitedCounters::programmable()) };
            Ok(true)
        }
        Err(Error::IllegalInstruction) => Ok(false),
        Err(error) => Err(error),
    }
}
