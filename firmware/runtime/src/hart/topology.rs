//! Immutable mapping from hardware hart IDs to compact storage indices.

use alloc::boxed::Box;
use core::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

use spin::Once;

use super::{HartId, HartIdError};

#[repr(C)]
pub(crate) struct HartEntry {
    pub(crate) raw_id: usize,
    pub(crate) stack_top: usize,
}

/// Entry-assembly view of the published hart table.
///
/// The pointer and count occupy the first two native words. Safe code cannot
/// modify them. Records are owned by Runtime for the firmware's lifetime.
#[repr(C)]
pub(crate) struct HartTable {
    entries: AtomicPtr<HartEntry>,
    count: AtomicUsize,
}

// The stack-independent entry path reads these native-word offsets directly.
const _: () = {
    assert!(core::mem::offset_of!(HartTable, entries) == 0);
    assert!(core::mem::offset_of!(HartTable, count) == size_of::<usize>());
    assert!(core::mem::offset_of!(HartEntry, raw_id) == 0);
    assert!(core::mem::offset_of!(HartEntry, stack_top) == size_of::<usize>());
    assert!(size_of::<HartEntry>() == 2 * size_of::<usize>());
};

/// The hart table consumed by the stack-independent entry path.
pub(crate) static HART_TABLE: HartTable = HartTable {
    entries: AtomicPtr::new(core::ptr::null_mut()),
    count: AtomicUsize::new(0),
};

static ENTRIES: Once<Box<[HartEntry]>> = Once::new();

pub(crate) fn publish(entries: Box<[HartEntry]>) {
    assert!(!ENTRIES.is_completed(), "hart topology already initialized");
    let entries = ENTRIES.call_once(|| entries);
    HART_TABLE.count.store(entries.len(), Ordering::Relaxed);
    HART_TABLE
        .entries
        .store(entries.as_ptr().cast_mut(), Ordering::Release);
}

pub(crate) fn from_raw(raw: usize) -> Result<HartId, HartIdError> {
    let entries = ENTRIES.get().ok_or(HartIdError::Unavailable)?;
    let index = entries
        .binary_search_by_key(&raw, |entry| entry.raw_id)
        .map_err(|_| HartIdError::Unknown)?;
    Ok(HartId { raw, index })
}

pub(crate) fn count() -> usize {
    ENTRIES.get().map_or(0, |entries| entries.len())
}

pub(crate) fn all() -> impl DoubleEndedIterator<Item = HartId> + ExactSizeIterator {
    ENTRIES
        .get()
        .map_or(&[][..], |entries| entries.as_ref())
        .iter()
        .enumerate()
        .map(|(index, entry)| HartId {
            raw: entry.raw_id,
            index,
        })
}
