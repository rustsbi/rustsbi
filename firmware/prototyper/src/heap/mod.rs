//! Prototyper's heap policy and boot-time registration.

#![forbid(unsafe_code)]

use self::allocator::HeapAllocator;
use runtime::heap::{self, HeapSlotList, SlotSizeMap};
use spin::Once;

mod allocator;

static HEAP_ALLOCATOR: Once<HeapAllocator> = Once::new();

/// Registers the firmware heap on the first call under the boot lock.
pub(crate) fn init() {
    if HEAP_ALLOCATOR.is_completed() {
        return;
    }
    let allocator = HEAP_ALLOCATOR.call_once(|| {
        let memory = heap::take_heap().expect("firmware heap already taken or too small");
        HeapAllocator::new(memory)
    });
    heap::register_global_heap_allocator(allocator, SlotSizeMap::rounded(HeapSlotList::GRANULE))
        .expect("firmware heap allocator already registered");
}

#[alloc_error_handler]
fn handle_alloc_error(layout: core::alloc::Layout) -> ! {
    panic!("firmware heap allocation failed: {layout:?}");
}
