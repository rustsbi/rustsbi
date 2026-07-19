//! Firmware heap storage and its global allocator.

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};

use buddy_system_allocator::LockedHeap;

use crate::config::HEAP_SIZE;

const MAX_ORDER: usize = 20;

#[repr(C, align(16))]
struct HeapStorage(UnsafeCell<[u8; HEAP_SIZE]>);

// SAFETY: the cold-entry claim initializes the allocator exactly once before
// any allocation. Later access is serialized by `LockedHeap`.
unsafe impl Sync for HeapStorage {}

#[used]
#[unsafe(link_section = ".bss.heap")]
static HEAP: HeapStorage = HeapStorage(UnsafeCell::new([0; HEAP_SIZE]));

#[global_allocator]
static ALLOCATOR: LockedHeap<MAX_ORDER> = LockedHeap::empty();

static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Initializes the one firmware allocator before the first owned boot copy.
pub(crate) fn initialize() {
    if INITIALIZED
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    // SAFETY: the successful claimant is the sole initializer. The static
    // storage is disjoint from allocator metadata and lives for the image.
    unsafe {
        ALLOCATOR
            .lock()
            .init(HEAP.0.get().cast::<u8>() as usize, HEAP_SIZE);
    }
}
