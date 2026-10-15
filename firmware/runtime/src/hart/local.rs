//! Runtime storage for immutable per-hart timer bindings.

use crate::{Error, Result};
use alloc::boxed::Box;
use spin::Once;

use super::HartId;

/// One permanently placed value per hart.
/// Values are ordinary shared data; hardware operations manage their own IRQ scope.
pub(crate) struct HartLocal<T> {
    slots: Once<Box<[Once<T>]>>,
}

impl<T> HartLocal<T> {
    /// Creates an empty storage pool without accessing hardware.
    pub(crate) const fn uninit() -> Self {
        Self { slots: Once::new() }
    }
}

impl<T: Send + Sync> HartLocal<T> {
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
        let hart = HartId::current().expect("hart-local access before topology publication");
        self.slots.get()?[hart.index()].get()
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

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn duplicate_publication_preserves_the_original_and_drops_both_values_once() {
        struct CountDrop<'a>(&'a AtomicUsize, usize);
        impl Drop for CountDrop<'_> {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }
        let drops = AtomicUsize::new(0);
        let slot = Once::new();
        publish(&slot, CountDrop(&drops, 1)).unwrap();
        assert!(matches!(
            publish(&slot, CountDrop(&drops, 2)),
            Err(Error::AlreadyInitialized)
        ));
        assert_eq!(slot.get().unwrap().1, 1);
        assert_eq!(drops.load(Ordering::Relaxed), 1);
        drop(slot);
        assert_eq!(drops.load(Ordering::Relaxed), 2);
    }
}
