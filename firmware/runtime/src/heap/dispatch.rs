//! Checked conversion between policy-owned slots and Rust allocations.

use super::{GlobalHeapAllocator, SlotSizeMap, memory::HeapArena};
use core::{alloc::Layout, ptr::NonNull};

pub(super) struct HeapDispatch<'a> {
    allocator: &'a dyn GlobalHeapAllocator,
    size_map: SlotSizeMap,
    arena: HeapArena,
}

impl<'a> HeapDispatch<'a> {
    pub(super) const fn new(
        allocator: &'a dyn GlobalHeapAllocator,
        size_map: SlotSizeMap,
        arena: HeapArena,
    ) -> Self {
        Self {
            allocator,
            size_map,
            arena,
        }
    }

    pub(super) fn alloc(&self, request: Layout) -> *mut u8 {
        let Some(slot_layout) = self.size_map.slot_layout(request) else {
            return core::ptr::null_mut();
        };
        let Ok(slot) = self.allocator.alloc(slot_layout) else {
            return core::ptr::null_mut();
        };
        if slot.size_bytes() != slot_layout.size()
            || !slot.start_addr().is_multiple_of(slot_layout.align())
            || !self.arena.contains(&slot)
        {
            self.allocator.dealloc(slot);
            return core::ptr::null_mut();
        }
        // Rust now owns these bytes; the exact slot is reconstructed when
        // GlobalAlloc returns them. There is no per-allocation header or table.
        slot.into_pointer().as_ptr()
    }

    /// Returns an allocation to the policy as its original slot.
    ///
    /// # Safety
    /// `pointer` and `request` must identify a live allocation returned by
    /// this dispatch. The caller transfers it back exactly once.
    pub(super) unsafe fn dealloc(&self, pointer: *mut u8, request: Layout) {
        let slot_layout = self
            .size_map
            .slot_layout(request)
            .expect("a live allocation has a representable slot layout");
        let pointer = NonNull::new(pointer).expect("a live allocation has a non-null pointer");
        // SAFETY: Every allocation was checked against this fixed map and
        // arena before its slot token was forgotten. GlobalAlloc's caller
        // supplies the original layout and relinquishes that allocation once.
        let slot = unsafe { self.arena.reclaim(pointer, slot_layout.size()) };
        self.allocator.dealloc(slot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heap::{GlobalHeapAllocator, HeapAllocError, HeapSlot, HeapSlotList};
    use alloc::boxed::Box;
    use core::mem::MaybeUninit;
    use spin::Mutex;

    #[repr(align(4096))]
    struct Backing([MaybeUninit<u8>; 4096]);

    fn arena_slot() -> HeapSlot<'static> {
        let backing = Box::leak(Box::new(Backing([MaybeUninit::uninit(); 4096])));
        HeapSlot::from_bytes(&mut backing.0).unwrap()
    }

    struct OneSlot {
        available: Mutex<Option<HeapSlot<'static>>>,
        returned: Mutex<Option<HeapSlot<'static>>>,
    }

    impl OneSlot {
        fn new(slot: HeapSlot<'static>) -> Self {
            Self {
                available: Mutex::new(Some(slot)),
                returned: Mutex::new(None),
            }
        }
    }

    impl GlobalHeapAllocator for OneSlot {
        fn alloc(&self, _slot_layout: Layout) -> Result<HeapSlot<'static>, HeapAllocError> {
            self.available.lock().take().ok_or(HeapAllocError)
        }

        fn dealloc(&self, slot: HeapSlot<'static>) {
            assert!(self.returned.lock().replace(slot).is_none());
        }
    }

    #[test]
    fn roundtrip_restores_the_original_arena_slot() {
        let memory = arena_slot();
        let arena = HeapArena::from_slot(&memory);
        let (slot, neighbor) = memory.split_at(HeapSlotList::GRANULE).unwrap();
        let policy = OneSlot::new(slot);
        let dispatch =
            HeapDispatch::new(&policy, SlotSizeMap::rounded(HeapSlotList::GRANULE), arena);
        let request = Layout::from_size_align(7, 8).unwrap();
        let pointer = dispatch.alloc(request);
        assert!(!pointer.is_null());
        // SAFETY: `pointer` names the live allocation just returned above.
        unsafe { dispatch.dealloc(pointer, request) };
        let returned = policy.returned.lock().take().unwrap();
        assert_eq!(returned.merge(neighbor).unwrap().size_bytes(), 4096);
    }

    #[test]
    fn rejects_a_slot_from_another_arena() {
        let memory = arena_slot();
        let arena = HeapArena::from_slot(&memory);
        let foreign = arena_slot().split_at(HeapSlotList::GRANULE).unwrap().0;
        let policy = OneSlot::new(foreign);
        let dispatch =
            HeapDispatch::new(&policy, SlotSizeMap::rounded(HeapSlotList::GRANULE), arena);
        assert!(dispatch.alloc(Layout::new::<u8>()).is_null());
        assert!(policy.returned.lock().is_some());
    }

    #[test]
    fn rejects_a_slot_larger_than_the_fixed_map() {
        let memory = arena_slot();
        let arena = HeapArena::from_slot(&memory);
        let oversized = memory.split_at(2 * HeapSlotList::GRANULE).unwrap().0;
        let policy = OneSlot::new(oversized);
        let dispatch =
            HeapDispatch::new(&policy, SlotSizeMap::rounded(HeapSlotList::GRANULE), arena);
        assert!(dispatch.alloc(Layout::new::<u8>()).is_null());
        assert_eq!(
            policy.returned.lock().as_ref().unwrap().size_bytes(),
            2 * HeapSlotList::GRANULE
        );
    }
}
