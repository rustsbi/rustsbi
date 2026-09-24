//! First-fit allocation and adjacent-block coalescing for the firmware heap.

#![forbid(unsafe_code)]

use core::alloc::Layout;
use runtime::heap::{GlobalHeapAllocator, HeapAllocError, HeapSlot, HeapSlotList};
use spin::Mutex;

// First-fit placement scans the free list under one lock. With a single arena,
// freeing needs at most three scans (two neighbors and the final miss). No
// allocation is performed while holding the lock. This is intended for the
// firmware's small heap, not a bounded-latency interrupt allocation path.
pub(super) struct HeapAllocator {
    free_slots: Mutex<HeapSlotList<'static>>,
}

impl HeapAllocator {
    pub(super) fn new(memory: HeapSlot<'static>) -> Self {
        // The linker places sbi_heap_start at a page boundary. Only a
        // trailing partial granule, if any, is excluded from the free list.
        let granule = HeapSlotList::GRANULE;
        assert!(memory.start_addr().is_multiple_of(granule));
        let size_bytes = memory.size_bytes() & !(granule - 1);
        assert!(size_bytes >= granule);
        let memory = if size_bytes == memory.size_bytes() {
            memory
        } else {
            memory.split_at(size_bytes).expect("nonempty heap tail").0
        };
        let mut free_slots = HeapSlotList::new();
        free_slots.push(memory).expect("aligned linker heap");
        Self {
            free_slots: Mutex::new(free_slots),
        }
    }
}

impl GlobalHeapAllocator for HeapAllocator {
    fn alloc(&self, slot_layout: Layout) -> Result<HeapSlot<'static>, HeapAllocError> {
        let align_bytes = slot_layout.align().max(HeapSlotList::GRANULE);
        let size_bytes = slot_layout.size();
        assert!(size_bytes.is_multiple_of(HeapSlotList::GRANULE));
        let mut free_slots = self.free_slots.lock();
        let mut offset_bytes = 0;
        let slot = free_slots
            .remove_if(|start_addr, available_bytes| {
                let Some(aligned_addr) = start_addr
                    .checked_add(align_bytes - 1)
                    .map(|address| address & !(align_bytes - 1))
                else {
                    return false;
                };
                let candidate_offset_bytes = aligned_addr - start_addr;
                let fits = candidate_offset_bytes
                    .checked_add(size_bytes)
                    .is_some_and(|needed_bytes| needed_bytes <= available_bytes);
                if fits {
                    offset_bytes = candidate_offset_bytes;
                }
                fits
            })
            .ok_or(HeapAllocError)?;
        let slot = if offset_bytes != 0 {
            let (prefix, slot) = slot.split_at(offset_bytes).expect("aligned interior split");
            free_slots.push(prefix).expect("aligned prefix");
            slot
        } else {
            slot
        };
        if slot.size_bytes() == size_bytes {
            return Ok(slot);
        }
        let (slot, suffix) = slot
            .split_at(size_bytes)
            .expect("granule-aligned interior split");
        free_slots.push(suffix).expect("aligned suffix");
        Ok(slot)
    }

    fn dealloc(&self, mut slot: HeapSlot<'static>) {
        let mut free_slots = self.free_slots.lock();
        // Allocation leaves disjoint free slots. Coalesce both neighbors before
        // inserting, so future requests can use the entire contiguous region.
        while let Some(neighbor) = free_slots.remove_if(|start_addr, size_bytes| {
            start_addr.checked_add(size_bytes) == Some(slot.start_addr())
                || slot.start_addr().checked_add(slot.size_bytes()) == Some(start_addr)
        }) {
            slot = slot
                .merge(neighbor)
                .expect("slot belongs to the heap arena");
        }
        free_slots.push(slot).expect("aligned returned slot");
    }
}
