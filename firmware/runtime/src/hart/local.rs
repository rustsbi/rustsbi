//! Synchronized storage indexed by hart identity.

#![forbid(unsafe_code)]

use spin::Mutex;

use super::HartId;
use crate::cfg::NUM_HART_MAX;

/// A fixed-capacity pool with one independently borrowed value per hart.
///
/// Recursive access is rejected instead of creating aliased mutable references
/// or waiting for a lock already held by the interrupted code.
pub struct HartLocal<T> {
    slots: [Mutex<Option<T>>; NUM_HART_MAX],
}

impl<T> HartLocal<T> {
    /// Creates an uninitialized storage pool.
    pub const fn uninit() -> Self {
        Self {
            slots: [const { Mutex::new(None) }; NUM_HART_MAX],
        }
    }

    /// Initializes one hart's slot exactly once.
    pub fn init(&self, hart: HartId, value: T) -> Result<(), HartLocalError> {
        let mut slot = self.slots[hart.0]
            .try_lock()
            .ok_or(HartLocalError::Borrowed)?;
        if slot.is_some() {
            return Err(HartLocalError::AlreadyInitialized);
        }
        *slot = Some(value);
        Ok(())
    }

    /// Runs `f` with exclusive access to the current hart's value.
    ///
    /// # Panics
    ///
    /// Panics if the slot is uninitialized or already borrowed, including
    /// recursive access and reentry from an interrupt handler.
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    pub fn with_current_mut<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        self.with_mut(super::current_hart(), f)
    }

    #[cfg(any(test, target_arch = "riscv32", target_arch = "riscv64"))]
    fn with_mut<R>(&self, hart: HartId, f: impl FnOnce(&mut T) -> R) -> R {
        let mut slot = self.slots[hart.0]
            .try_lock()
            .expect("hart-local slot is already borrowed");
        f(slot.as_mut().expect("hart-local slot is uninitialized"))
    }
}

/// Failure to initialize a hart-local slot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HartLocalError {
    /// The slot already contains a value.
    AlreadyInitialized,
    /// An operation currently holds the slot.
    Borrowed,
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use core::sync::atomic::{AtomicUsize, Ordering};
    use std::panic::{AssertUnwindSafe, catch_unwind};

    #[test]
    fn recursive_access_is_rejected_and_releases_the_borrow() {
        let pool = HartLocal::uninit();
        let hart = HartId::from_raw(0).unwrap();
        pool.init(hart, 7).unwrap();
        assert!(
            catch_unwind(AssertUnwindSafe(|| {
                pool.with_mut(hart, |_| pool.with_mut(hart, |_| ()));
            }))
            .is_err()
        );
        pool.with_mut(hart, |value| *value += 1);
        assert_eq!(pool.with_mut(hart, |value| *value), 8);
    }

    #[test]
    fn duplicate_initialization_preserves_the_original_and_drops_values() {
        struct CountDrop<'a>(&'a AtomicUsize);
        impl Drop for CountDrop<'_> {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }
        let drops = AtomicUsize::new(0);
        let pool = HartLocal::uninit();
        let hart = HartId::from_raw(0).unwrap();
        pool.init(hart, CountDrop(&drops)).unwrap();
        assert_eq!(
            pool.init(hart, CountDrop(&drops)),
            Err(HartLocalError::AlreadyInitialized)
        );
        assert_eq!(drops.load(Ordering::Relaxed), 1);
        drop(pool);
        assert_eq!(drops.load(Ordering::Relaxed), 2);
    }
}
