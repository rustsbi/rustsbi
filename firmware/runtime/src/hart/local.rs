//! Shared storage indexed by Runtime's enabled hart topology.

use crate::{Error, Result};
use alloc::boxed::Box;
use spin::Once;

use super::HartId;

/// One permanently placed value per hart.
/// Values are ordinary shared data; hardware operations manage their own IRQ scope.
pub struct HartLocal<T> {
    slots: Once<Box<[Once<T>]>>,
}

impl<T> HartLocal<T> {
    /// Creates empty per-hart storage without accessing hardware.
    pub const fn new() -> Self {
        Self::uninit()
    }
    /// Creates an empty storage pool without accessing hardware.
    pub(crate) const fn uninit() -> Self {
        Self { slots: Once::new() }
    }
}

impl<T> Default for HartLocal<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Send + Sync> HartLocal<T> {
    /// Initializes every enabled hart's software record once.
    /// Duplicate calls preserve the existing records without running `initialize`.
    pub fn init_all(&self, mut initialize: impl FnMut(HartId) -> T) -> Result<()> {
        if HartId::count() == 0 {
            return Err(Error::NotReady);
        }
        let mut installed = false;
        self.slots.call_once(|| {
            installed = true;
            HartId::all()
                .map(|hart| {
                    let slot = Once::new();
                    slot.call_once(|| initialize(hart));
                    slot
                })
                .collect()
        });
        if installed {
            Ok(())
        } else {
            Err(Error::AlreadyInitialized)
        }
    }

    /// Borrows an initialized record by its validated hart identifier.
    pub fn get(&self, hart: HartId) -> Result<&T> {
        self.slots
            .get()
            .and_then(|slots| slots[hart.index()].get())
            .ok_or(Error::NotReady)
    }

    /// Iterates initialized records in hardware-ID order.
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &T> {
        self.slots
            .get()
            .map_or(&[][..], |slots| slots.as_ref())
            .iter()
            .filter_map(Once::get)
    }
    /// Borrows this hart's initialized value.
    ///
    /// # Panics
    ///
    /// Panics before topology publication or if this hart is uninitialized.
    pub(crate) fn get_current(&self) -> &T {
        self.try_get_current()
            .expect("hart-local slot is uninitialized")
    }

    pub(crate) fn try_get_current(&self) -> Option<&T> {
        let slots = self.slots.get()?;
        let hart = HartId::current().ok()?;
        slots[hart.index()].get()
    }

    /// Publishes an already constructed value in the current hart's slot.
    /// Initialization takes a value rather than a callback.
    /// Duplicate publication preserves the original and drops the incoming value.
    pub(crate) fn try_init_current(&self, value: T) -> Result<&T> {
        let hart = super::current_hart();
        let slots = self
            .slots
            .call_once(|| HartId::all().map(|_| Once::new()).collect());
        publish(&slots[hart.index()], value)
    }
}

fn publish<T>(slot: &Once<T>, value: T) -> Result<&T> {
    let mut initialized = false;
    let stored = slot.call_once(|| {
        initialized = true;
        value
    });
    if initialized {
        Ok(stored)
    } else {
        Err(Error::AlreadyInitialized)
    }
}
