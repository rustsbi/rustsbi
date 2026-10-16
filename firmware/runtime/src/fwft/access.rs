//! Initialization and scoped current-hart hardware access.

use super::{Feature, Features, Fwft, State, Value};
use crate::{Error, Result, hart::Hart, irq};
use csr::machine;
use spin::Mutex;

impl Fwft {
    /// Initializes hardware defaults once during initial hart configuration.
    pub(crate) fn init_current(&self, hart: &Hart) -> Result<()> {
        if self.local.try_get_current().is_some() {
            return Err(Error::AlreadyInitialized);
        }
        self.local
            .try_init_current(Mutex::new(State::initialize(hart)?))?;
        Ok(())
    }

    /// Runs one feature transaction with local IRQs disabled.
    /// Returns `NotReady` before initialization or outside Runtime's machine entry,
    /// and `Busy` on recursive access.
    /// The view cannot escape or move to another hart.
    /// The closure must not wait for local IRQs or locks held by an interrupted caller.
    pub fn with_current<T>(
        &self,
        operation: impl FnOnce(&mut Features<'_>) -> Result<T>,
    ) -> Result<T> {
        let state = self.local.try_get_current().ok_or(Error::NotReady)?;
        if !crate::boot::traps_ready() || usize::from(machine::Scratch::read()) != 0 {
            return Err(Error::NotReady);
        }
        irq::with_local_disabled(|irq| {
            let mut state = state.try_lock().ok_or(Error::Busy)?;
            operation(&mut Features {
                state: &mut state,
                _irq: irq,
            })
        })
    }

    /// Restores committed hardware values before handing off a fresh context.
    pub(crate) fn restore_current(&self) -> Result<()> {
        self.with_current(|features| features.state.restore())
    }
}

impl Features<'_> {
    /// Reads the actual feature value without changing hardware.
    pub fn get(&self, feature: Feature) -> Result<Value> {
        self.state.get(feature)
    }

    /// Updates a feature, preserving unrelated CSR fields and synchronizing translations.
    /// Errors leave its hardware value and committed restoration value unchanged.
    ///
    /// # Panics
    /// Panics if hardware cannot restore the old value after a failed update.
    pub fn set(&mut self, feature: Feature, value: Value) -> Result<()> {
        self.state.set(feature, value)
    }
}
