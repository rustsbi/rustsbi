use crate::cfg::HEAP_SIZE;
use buddy_system_allocator::LockedHeap;

#[unsafe(link_section = ".bss.heap")]
static mut HEAP: [u8; HEAP_SIZE] = [0; HEAP_SIZE];

#[global_allocator]
static HEAP_ALLOCATOR: LockedHeap<15> = LockedHeap::<15>::empty();

pub fn sbi_heap_init() {
    unsafe {
        HEAP_ALLOCATOR
            .lock()
            .init(HEAP.as_ptr() as usize, HEAP_SIZE);
    }
}

#[alloc_error_handler]
pub fn handle_alloc_error(layout: core::alloc::Layout) -> ! {
    panic!("Heap allocation error, layout = {:?}", layout);
}
