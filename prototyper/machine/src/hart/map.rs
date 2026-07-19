//! Bounded hart identity mapping and exclusive hart-local storage.

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use crate::config::HART_CAPACITY;

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
mod entry;

use alloc::boxed::Box;
use alloc::vec::Vec;
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
pub(crate) use entry::entry_index;

const MAP_EMPTY: u32 = 0;
const MAP_WRITING: u32 = 1;
const MAP_READY: u32 = 2;

/// An unvalidated architectural hart selection request.
///
/// The value carries no dense index or access authority. Machine protocol code
/// resolves it against one immutable admitted-hart map and one locked HSM
/// snapshot before changing state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HartTargets {
    kind: HartTargetsKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HartTargetsKind {
    AllAvailable,
    Selected {
        hart_bits: usize,
        base_hart_id: usize,
    },
}

impl HartTargets {
    /// Selects every hart available to the current supervisor context.
    pub const fn all_available() -> Self {
        Self {
            kind: HartTargetsKind::AllAvailable,
        }
    }

    /// Selects the architectural IDs named by an SBI-style bit mask and base.
    pub const fn selected(hart_bits: usize, base_hart_id: usize) -> Self {
        Self {
            kind: HartTargetsKind::Selected {
                hart_bits,
                base_hart_id,
            },
        }
    }

    pub(crate) const fn selected_parts(self) -> Option<(usize, usize)> {
        match self.kind {
            HartTargetsKind::AllAvailable => None,
            HartTargetsKind::Selected {
                hart_bits,
                base_hart_id,
            } => Some((hart_bits, base_hart_id)),
        }
    }
}

/// An architectural hart identity.
///
/// IDs may be sparse and must never directly index per-hart storage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
struct HartId(usize);

/// A checked dense position in machine-owned per-hart storage.
///
/// Only `HartMap` creates an index, after proving it is below the admitted hart
/// count. The integer never crosses the machine boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct HartIndex(usize);

/// Immutable one-to-one physical-ID mapping prepared before runtime release.
///
/// Every admitted `HartId` maps to one in-bounds `HartIndex`, and duplicate IDs
/// are rejected so two physical harts can never own the same storage entry.
struct HartMap<const CAPACITY: usize> {
    ids: [HartId; CAPACITY],
    len: usize,
}

impl<const CAPACITY: usize> HartMap<CAPACITY> {
    fn new(ids: &[usize], boot_hart: usize) -> Result<Self, HartMapError> {
        if ids.is_empty() {
            return Err(HartMapError::Empty);
        }
        if ids.len() > CAPACITY {
            return Err(HartMapError::TooManyHarts);
        }

        let mut map = Self {
            ids: [HartId(0); CAPACITY],
            len: 0,
        };
        for &raw_id in ids {
            if map.resolve(raw_id).is_some() {
                return Err(HartMapError::DuplicateHart);
            }
            map.ids[map.len] = HartId(raw_id);
            map.len += 1;
        }

        if map.resolve(boot_hart).is_none() {
            return Err(HartMapError::BootHartMissing);
        }
        Ok(map)
    }

    fn resolve(&self, raw_id: usize) -> Option<HartIndex> {
        self.ids[..self.len]
            .iter()
            .position(|id| *id == HartId(raw_id))
            .map(HartIndex)
    }
}

/// One immutable hart map with an explicit one-time publication boundary.
///
/// The initializer builds and validates a temporary `HartMap` before claiming
/// this cell. It then writes every non-atomic field and performs the sole
/// Release transition to `MAP_READY`. A reader must observe that state with
/// Acquire ordering before touching `len` or `ids`.
#[repr(C)]
struct PublishedHartMap<const CAPACITY: usize> {
    state: AtomicU32,
    len: UnsafeCell<usize>,
    ids: UnsafeCell<[HartId; CAPACITY]>,
}

static HART_MAP: PublishedHartMap<HART_CAPACITY> = PublishedHartMap::new();

pub(crate) fn publish(ids: &[usize], boot_hart: usize) -> Result<(), HartMapError> {
    HART_MAP.publish(ids, boot_hart)
}

pub(crate) fn resolve(raw_id: usize) -> Option<usize> {
    HART_MAP.resolve(raw_id).map(|index| index.0)
}

// SAFETY: only the successful EMPTY -> WRITING claimant mutates the cell, and
// no mutation occurs after Release publication. Readers access non-atomic data
// only after an Acquire observation of READY.
unsafe impl<const CAPACITY: usize> Sync for PublishedHartMap<CAPACITY> {}

impl<const CAPACITY: usize> PublishedHartMap<CAPACITY> {
    const fn new() -> Self {
        Self {
            state: AtomicU32::new(MAP_EMPTY),
            len: UnsafeCell::new(0),
            ids: UnsafeCell::new([HartId(0); CAPACITY]),
        }
    }

    fn publish(&self, ids: &[usize], boot_hart: usize) -> Result<(), HartMapError> {
        let map = HartMap::new(ids, boot_hart)?;
        self.state
            .compare_exchange(MAP_EMPTY, MAP_WRITING, Ordering::Acquire, Ordering::Relaxed)
            .map_err(|_| HartMapError::AlreadyPublished)?;

        // SAFETY: this caller uniquely changed EMPTY to WRITING. The fields are
        // unreachable to readers until the Release publication below, and are
        // immutable thereafter.
        unsafe {
            self.len.get().write(map.len);
            self.ids.get().write(map.ids);
        }
        self.state.store(MAP_READY, Ordering::Release);
        Ok(())
    }

    fn resolve(&self, raw_id: usize) -> Option<HartIndex> {
        if self.state.load(Ordering::Acquire) != MAP_READY {
            return None;
        }

        // SAFETY: the Acquire load observed the sole Release publication, so
        // both fields are initialized and immutable for the firmware lifetime.
        let (len, ids) = unsafe { (*self.len.get(), &*self.ids.get()) };
        ids[..len]
            .iter()
            .position(|id| *id == HartId(raw_id))
            .map(HartIndex)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HartMapError {
    Empty,
    TooManyHarts,
    DuplicateHart,
    BootHartMissing,
    AlreadyPublished,
}

/// One dynamically checked exclusive-borrow boundary for a subsystem slot.
///
/// Separate subsystem arrays own separate `LocalSlot` values; there is no
/// combined per-hart context that grants unrelated authority.
struct LocalSlot<T> {
    borrowed: AtomicBool,
    value: UnsafeCell<T>,
}

// SAFETY: the atomic flag permits at most one live mutable guard. Moving that
// guard between execution contexts is valid only when `T: Send`.
unsafe impl<T: Send> Sync for LocalSlot<T> {}

impl<T> LocalSlot<T> {
    const fn new(value: T) -> Self {
        Self {
            borrowed: AtomicBool::new(false),
            value: UnsafeCell::new(value),
        }
    }

    fn try_claim(&self) -> bool {
        self.borrowed
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
    }
}

/// Failure while selecting or exclusively borrowing current-hart state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HartLocalError {
    /// No initialized machine hart corresponds to the current execution.
    InvalidHart,
    /// The same hart has re-entered this subsystem while its guard is live.
    Borrowed,
}

/// Separate exclusive state of type `T` for every admitted hart.
pub struct HartLocal<T> {
    slots: Box<[LocalSlot<T>]>,
}

impl<T> HartLocal<T> {
    /// Constructs boot-owned per-hart state in dense admitted-hart order.
    pub fn new(values: Vec<T>) -> Result<Self, HartLocalError> {
        if values.is_empty() || values.len() > HART_CAPACITY {
            return Err(HartLocalError::InvalidHart);
        }
        Ok(Self {
            slots: values
                .into_iter()
                .map(LocalSlot::new)
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        })
    }

    /// Exclusively borrows only the calling hart's subsystem value.
    pub fn current(&self) -> Result<HartLocalGuard<'_, T>, HartLocalError> {
        let interrupts = LocalInterrupts::disable();
        let Some(index) = crate::trap::current_index() else {
            interrupts.restore();
            return Err(HartLocalError::InvalidHart);
        };
        let Some(slot) = self.slots.get(index) else {
            interrupts.restore();
            return Err(HartLocalError::InvalidHart);
        };
        if !slot.try_claim() {
            interrupts.restore();
            return Err(HartLocalError::Borrowed);
        }
        Ok(HartLocalGuard { slot, interrupts })
    }
}

/// One lifetime-bound exclusive borrow of current-hart subsystem state.
pub struct HartLocalGuard<'local, T> {
    slot: &'local LocalSlot<T>,
    interrupts: LocalInterrupts,
}

impl<T> Deref for HartLocalGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        // SAFETY: construction acquired this slot's exclusive-borrow flag.
        unsafe { &*self.slot.value.get() }
    }
}

impl<T> DerefMut for HartLocalGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: the live guard uniquely owns the slot until `Drop`.
        unsafe { &mut *self.slot.value.get() }
    }
}

impl<T> Drop for HartLocalGuard<'_, T> {
    fn drop(&mut self) {
        // Protocol invariant: release the dynamic borrow before an interrupt
        // can re-enter this subsystem on the same hart.
        self.slot.borrowed.store(false, Ordering::Release);
        self.interrupts.restore();
    }
}

struct LocalInterrupts {
    previous: usize,
}

impl LocalInterrupts {
    fn disable() -> Self {
        Self {
            previous: super::arch::mask_all_interrupts(),
        }
    }

    fn restore(&self) {
        super::arch::restore_all_interrupts(self.previous)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparse_ids_map_to_dense_private_indices() {
        let map = HartMap::<3>::new(&[0, 8, 0x1000], 8).unwrap();

        assert_eq!(map.resolve(0), Some(HartIndex(0)));
        assert_eq!(map.resolve(8), Some(HartIndex(1)));
        assert_eq!(map.resolve(0x1000), Some(HartIndex(2)));
        assert_eq!(map.resolve(1), None);
    }

    #[test]
    fn rejects_invalid_admission_sets() {
        assert!(matches!(
            HartMap::<2>::new(&[], 0),
            Err(HartMapError::Empty)
        ));
        assert!(matches!(
            HartMap::<1>::new(&[0, 1], 0),
            Err(HartMapError::TooManyHarts)
        ));
        assert!(matches!(
            HartMap::<2>::new(&[3, 3], 3),
            Err(HartMapError::DuplicateHart)
        ));
        assert!(matches!(
            HartMap::<2>::new(&[2, 4], 3),
            Err(HartMapError::BootHartMissing)
        ));
    }

    #[test]
    fn slot_rejects_aliasing_and_reopens_after_drop() {
        let local = HartLocal::new(alloc::vec![4]).unwrap();
        let mut first = local.current().unwrap();
        assert_eq!(local.current().err(), Some(HartLocalError::Borrowed));
        *first += 3;
        drop(first);

        assert_eq!(*local.current().unwrap(), 7);
    }

    #[test]
    fn publishes_a_complete_map_exactly_once() {
        let published = PublishedHartMap::<3>::new();
        assert_eq!(published.resolve(8), None);

        published.publish(&[0, 8, 0x1000], 8).unwrap();
        assert_eq!(published.resolve(8), Some(HartIndex(1)));
        assert_eq!(published.resolve(9), None);
        assert_eq!(
            published.publish(&[0, 8, 0x1000], 8),
            Err(HartMapError::AlreadyPublished)
        );
    }

    #[test]
    fn failed_validation_publishes_nothing() {
        let published = PublishedHartMap::<2>::new();
        assert_eq!(
            published.publish(&[4, 4], 4),
            Err(HartMapError::DuplicateHart)
        );
        assert_eq!(published.resolve(4), None);

        published.publish(&[4, 9], 4).unwrap();
        assert_eq!(published.resolve(9), Some(HartIndex(1)));
    }

    #[test]
    fn hart_local_rejects_same_hart_reentry_and_reopens_after_drop() {
        let local = HartLocal::new(alloc::vec![4usize, 9]).unwrap();
        let mut first = local.current().unwrap();
        assert!(matches!(local.current(), Err(HartLocalError::Borrowed)));
        *first = 7;
        drop(first);
        assert_eq!(*local.current().unwrap(), 7);
    }

    #[test]
    fn hart_local_rejects_empty_or_over_capacity_storage() {
        assert!(matches!(
            HartLocal::<usize>::new(Vec::new()),
            Err(HartLocalError::InvalidHart)
        ));
        assert!(matches!(
            HartLocal::new(alloc::vec![0usize; HART_CAPACITY + 1]),
            Err(HartLocalError::InvalidHart)
        ));
    }
}
