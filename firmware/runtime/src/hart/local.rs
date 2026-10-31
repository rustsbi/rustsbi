//! Per-hart storage indexed by Runtime's enabled hart topology.

use crate::{Error, Result};
use alloc::boxed::Box;

use super::HartId;

/// A value for each hart in Runtime's published topology.
///
/// Runtime publishes the topology once during boot and never changes it,
/// so a [`HartId`] identifies the same value for the lifetime of this collection.
///
/// Values are shared data, including when accessed from another hart.
/// Callers must synchronize mutation of `T` themselves.
/// `PerHart<T>` is `Sync` exactly when `T` is `Sync`.
pub struct PerHart<T> {
    slots: Box<[T]>,
}

impl<T> PerHart<T> {
    /// Builds one value per enabled hart in [`HartId::all`] order.
    ///
    /// Returns [`Error::NotReady`] before Runtime publishes the hart topology.
    /// The initializer runs on the calling hart and receives the hart
    /// whose value it is building.
    /// The values become available only after every initializer call has completed.
    ///
    /// Construction allocates memory.
    /// Allocation failure follows the configured global allocator's error handling
    /// and is not returned here.
    pub fn new(init: impl FnMut(HartId) -> T) -> Result<Self> {
        if HartId::count() == 0 {
            return Err(Error::NotReady);
        }
        let slots: Box<[T]> = HartId::all().map(init).collect();
        Ok(Self { slots })
    }

    /// Borrows the value for a hart validated by Runtime's topology.
    pub fn get(&self, hart: HartId) -> &T {
        &self.slots[hart.index()]
    }

    /// Iterates over values in ascending hardware-ID order.
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &T> {
        self.slots.iter()
    }
}
