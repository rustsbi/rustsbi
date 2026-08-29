//! Firmware heap allocator.

use crate::cfg::HEAP_SIZE;
use buddy_system_allocator::LockedHeap;
use spin::Once;

#[unsafe(link_section = ".bss.heap")]
static RAW_HEAP: [u8; HEAP_SIZE] = [0; HEAP_SIZE];

const BUDDY_MAX_ORDER: usize = 20;
#[global_allocator]
static HEAP_ALLOCATOR: LockedHeap<BUDDY_MAX_ORDER> = LockedHeap::<BUDDY_MAX_ORDER>::empty();

static HEAP_INIT: Once<()> = Once::new();

/// Initializes the global heap allocator.
///
/// Must be called exactly once, on the boot hart, before any allocation.
pub fn init() {
    HEAP_INIT.call_once(|| {
        // SAFETY: `RAW_HEAP` is a BSS array whose address is handed to the
        // allocator exactly once; after this call the allocator owns the
        // region and no Rust reference to it is created again.
        unsafe { HEAP_ALLOCATOR.lock().init(raw_heap_addr(), HEAP_SIZE) };
    });
}

fn raw_heap_addr() -> usize {
    core::ptr::addr_of!(RAW_HEAP) as usize
}

#[alloc_error_handler]
pub fn handle_alloc_error(layout: core::alloc::Layout) -> ! {
    error!("Heap stats:");
    {
        let heap = HEAP_ALLOCATOR.lock();
        error!("\tTotal size: {}", heap.stats_total_bytes());
        error!("\tRequested size: {}", heap.stats_alloc_user());
        error!("\tAllocated size: {}", heap.stats_alloc_actual());
        error!(
            "Currently the heap only support allocate buffer with max length {} bytes.",
            1 << (BUDDY_MAX_ORDER - 1)
        );
    }
    panic!("Heap allocation error, layout = {:?}", layout);
}
