//! Linker heap acquisition and the firmware global allocator.

use super::{
    GlobalHeapAllocator, HeapAlreadyRegistered, HeapSlot, SlotSizeMap, dispatch::HeapDispatch,
    memory::HeapArena,
};
use core::{
    alloc::{GlobalAlloc, Layout},
    ptr::NonNull,
    sync::atomic::{AtomicBool, Ordering},
};
use spin::Once;

unsafe extern "C" {
    static sbi_heap_start: u8;
    static sbi_heap_end: u8;
}

static HEAP_TAKEN: AtomicBool = AtomicBool::new(false);
static HEAP_ARENA: Once<HeapArena> = Once::new();
static GLOBAL_HEAP_ALLOCATOR: Once<HeapDispatch<'static>> = Once::new();

/// Takes the linker-reserved heap exactly once across all harts.
pub fn take_heap() -> Option<HeapSlot<'static>> {
    if HEAP_TAKEN
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return None;
    }
    let start_addr = core::ptr::addr_of!(sbi_heap_start) as usize;
    let end_addr = core::ptr::addr_of!(sbi_heap_end) as usize;
    let pointer = NonNull::new(start_addr as *mut u8)?;
    // SAFETY: The linker reserves this writable range outside all Rust objects.
    // The atomic gate transfers it only once, before any heap allocation.
    let slot = unsafe { HeapSlot::from_raw(pointer, end_addr.checked_sub(start_addr)?)? };
    HEAP_ARENA.call_once(|| HeapArena::from_slot(&slot));
    Some(slot)
}

/// Registers the initialized allocator before the first firmware allocation.
///
/// Call this after [`take_heap`]. Every subsequent registration is rejected,
/// even for the same instance.
pub fn register_global_heap_allocator(
    allocator: &'static dyn GlobalHeapAllocator,
    size_map: SlotSizeMap,
) -> Result<(), HeapAlreadyRegistered> {
    let arena = *HEAP_ARENA.get().expect("firmware heap must be taken first");
    let mut installed = false;
    GLOBAL_HEAP_ALLOCATOR.call_once(|| {
        installed = true;
        HeapDispatch::new(allocator, size_map, arena)
    });
    if installed {
        Ok(())
    } else {
        Err(HeapAlreadyRegistered)
    }
}

struct AllocDispatch;

#[global_allocator]
static ALLOC_DISPATCH: AllocDispatch = AllocDispatch;

// SAFETY: Dispatch checks that policy slots belong to the linker arena and
// match the immutable size map. HeapSlot cannot be duplicated in safe code.
// Firmware aborts on panic, so policy failures cannot unwind through this API.
unsafe impl GlobalAlloc for AllocDispatch {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        GLOBAL_HEAP_ALLOCATOR
            .get()
            .map_or(core::ptr::null_mut(), |allocator| allocator.alloc(layout))
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        let allocator = GLOBAL_HEAP_ALLOCATOR
            .get()
            .expect("heap allocation requires registration");
        // SAFETY: GlobalAlloc's caller guarantees a live allocation with the
        // original layout. The registered allocator cannot be replaced.
        unsafe { allocator.dealloc(pointer, layout) };
    }
}
