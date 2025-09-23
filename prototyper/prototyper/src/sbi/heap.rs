use crate::cfg::HEAP_SIZE;
use buddy_system_allocator::LockedHeap;

#[unsafe(link_section = ".bss.heap")]
static mut HEAP: [u8; HEAP_SIZE] = [0; HEAP_SIZE];

const BUDDY_MAX_ORDER: usize = 20;
#[global_allocator]
static HEAP_ALLOCATOR: LockedHeap<BUDDY_MAX_ORDER> = LockedHeap::<BUDDY_MAX_ORDER>::empty();

pub fn sbi_heap_init() {
    unsafe {
        HEAP_ALLOCATOR
            .lock()
            .init(HEAP.as_ptr() as usize, HEAP_SIZE);
    }
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
