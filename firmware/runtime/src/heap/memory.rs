//! Exclusive heap slots and allocation-free storage for free slots.

use core::{
    marker::PhantomData,
    mem::MaybeUninit,
    ptr::NonNull,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT_ARENA_ID: AtomicUsize = AtomicUsize::new(0);

/// Exclusive ownership of a writable heap range for the lifetime of its backing.
///
/// Safe code cannot copy slots or construct them from addresses. Splitting
/// consumes ownership, and merging requires adjacent parts of the same initial slot.
/// Dropping a slot does not return it to an allocator. Borrowed backing can be
/// reused once all its slots and lists are gone.
#[derive(Debug)]
pub struct HeapSlot<'a> {
    pub(super) pointer: NonNull<u8>,
    pub(super) size_bytes: usize,
    arena_id: usize,
    backing: PhantomData<&'a mut [MaybeUninit<u8>]>,
}

// SAFETY: A slot owns its bytes exclusively, exposes no references, and its
// backing memory outlives the slot. Moving it transfers that ownership.
unsafe impl Send for HeapSlot<'_> {}

/// Bounds and identity of the one linker-owned heap arena.
#[derive(Clone, Copy)]
pub(super) struct HeapArena {
    start_addr: usize,
    end_addr: usize,
    id: usize,
}

impl HeapArena {
    pub(super) fn from_slot(slot: &HeapSlot<'static>) -> Self {
        Self {
            start_addr: slot.start_addr(),
            end_addr: slot.start_addr() + slot.size_bytes(),
            id: slot.arena_id,
        }
    }

    pub(super) fn contains(self, slot: &HeapSlot<'_>) -> bool {
        slot.arena_id == self.id
            && slot.start_addr() >= self.start_addr
            && slot
                .start_addr()
                .checked_add(slot.size_bytes())
                .is_some_and(|end_addr| end_addr <= self.end_addr)
    }

    /// Recreates the slot relinquished to a Rust allocation.
    ///
    /// # Safety
    /// `pointer` must identify a live allocation returned from this arena,
    /// with exactly `size_bytes` owned bytes. The caller transfers ownership
    /// back exactly once and must not use the allocation afterwards.
    pub(super) unsafe fn reclaim(
        self,
        pointer: NonNull<u8>,
        size_bytes: usize,
    ) -> HeapSlot<'static> {
        let slot = HeapSlot {
            pointer,
            size_bytes,
            arena_id: self.id,
            backing: PhantomData,
        };
        assert!(
            self.contains(&slot),
            "heap deallocation is outside its arena"
        );
        slot
    }
}

impl<'a> HeapSlot<'a> {
    /// Borrows nonempty storage exclusively, without rounding its size or address.
    pub fn from_bytes(storage: &'a mut [MaybeUninit<u8>]) -> Option<Self> {
        let pointer = NonNull::new(storage.as_mut_ptr().cast())?;
        // SAFETY: The exclusive borrow grants access to every byte for 'a.
        unsafe { Self::from_raw(pointer, storage.len()) }
    }

    /// Creates the initial slot for exclusive, writable storage.
    ///
    /// # Safety
    /// The range starting at `pointer` and spanning `size_bytes` must lie in one
    /// live, writable backing region and remain valid for `'a`. `size_bytes`
    /// must not exceed `isize::MAX` for in-bounds pointer arithmetic. The caller
    /// must transfer exclusive access to that range, with no other references
    /// or owners.
    pub(super) unsafe fn from_raw(pointer: NonNull<u8>, size_bytes: usize) -> Option<Self> {
        if size_bytes == 0
            || size_bytes > isize::MAX as usize
            || (pointer.as_ptr() as usize)
                .checked_add(size_bytes)
                .is_none()
        {
            return None;
        }
        // Separate initial slots may occupy disjoint parts of one backing
        // allocation. Their addresses alone do not authorize merging them.
        let arena_id = NEXT_ARENA_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .expect("heap arena identities exhausted");
        Some(Self {
            pointer,
            size_bytes,
            arena_id,
            backing: PhantomData,
        })
    }

    /// Returns the start address for placement decisions, without memory access.
    pub fn start_addr(&self) -> usize {
        self.pointer.as_ptr() as usize
    }

    /// Returns the owned size in bytes.
    pub fn size_bytes(&self) -> usize {
        self.size_bytes
    }

    /// Relinquishes the slot token when Rust takes ownership of its bytes.
    pub(super) fn into_pointer(self) -> NonNull<u8> {
        self.pointer
    }

    /// Splits at a nonzero interior byte offset, returning ownership on failure.
    pub fn split_at(self, offset_bytes: usize) -> Result<(Self, Self), Self> {
        if offset_bytes == 0 || offset_bytes >= self.size_bytes {
            return Err(self);
        }
        // SAFETY: Both nonempty ranges remain within the original owned region.
        let right_pointer =
            unsafe { NonNull::new_unchecked(self.pointer.as_ptr().add(offset_bytes)) };
        let right = Self {
            pointer: right_pointer,
            size_bytes: self.size_bytes - offset_bytes,
            arena_id: self.arena_id,
            backing: PhantomData,
        };
        let left = Self {
            size_bytes: offset_bytes,
            ..self
        };
        Ok((left, right))
    }

    /// Merges adjacent slots derived from the same initial slot, in either order.
    pub fn merge(self, other: Self) -> Result<Self, (Self, Self)> {
        if self.arena_id != other.arena_id {
            return Err((self, other));
        }
        if other.start_addr() < self.start_addr() {
            return other.merge(self);
        }
        if self.start_addr().checked_add(self.size_bytes) != Some(other.start_addr()) {
            return Err((self, other));
        }
        Ok(Self {
            size_bytes: self.size_bytes + other.size_bytes,
            ..self
        })
    }
}

struct FreeNode {
    size_bytes: usize,
    arena_id: usize,
    next: Option<NonNull<FreeNode>>,
}

/// An allocation-free, unordered collection owning free heap slots.
///
/// Insertion consumes a slot and removal returns it. The collection provides
/// storage only; policy code decides which slots to select, split, or merge.
/// Stored slots must obey [`Self::GRANULE`]; this constraint does not apply to
/// slots managed with a different data structure.
pub struct HeapSlotList<'a> {
    head: Option<NonNull<FreeNode>>,
    backing: PhantomData<HeapSlot<'a>>,
}

// SAFETY: The list exclusively owns every node and its backing slot. Mutation
// requires &mut self; transferring the list transfers all slot ownership.
unsafe impl Send for HeapSlotList<'_> {}

impl<'a> HeapSlotList<'a> {
    /// Required size multiple and alignment in bytes for this intrusive list.
    pub const GRANULE: usize = size_of::<FreeNode>().next_power_of_two();

    /// Creates an empty list.
    pub const fn new() -> Self {
        Self {
            head: None,
            backing: PhantomData,
        }
    }

    /// Inserts a slot without allocating; returns it if its layout is unsuitable.
    pub fn push(&mut self, slot: HeapSlot<'a>) -> Result<(), HeapSlot<'a>> {
        if !slot.start_addr().is_multiple_of(Self::GRANULE)
            || !slot.size_bytes().is_multiple_of(Self::GRANULE)
        {
            return Err(slot);
        }
        let pointer = slot.pointer.cast::<FreeNode>();
        // SAFETY: Slot granularity guarantees alignment and space for FreeNode.
        // Consuming the slot grants exclusive access; no other owner can see the node.
        unsafe {
            pointer.as_ptr().write(FreeNode {
                size_bytes: slot.size_bytes,
                arena_id: slot.arena_id,
                next: self.head,
            });
        }
        self.head = Some(pointer);
        Ok(())
    }

    /// Removes the first slot whose address and size satisfy `predicate_fn`.
    pub fn remove_if(
        &mut self,
        mut predicate_fn: impl FnMut(usize, usize) -> bool,
    ) -> Option<HeapSlot<'a>> {
        let mut link = &mut self.head;
        while let Some(mut pointer) = *link {
            // SAFETY: Every link points to a live, exclusively list-owned node.
            // &mut self prevents another traversal or mutation until this call returns.
            let node = unsafe { pointer.as_mut() };
            if predicate_fn(pointer.as_ptr() as usize, node.size_bytes) {
                *link = node.next;
                return Some(HeapSlot {
                    pointer: pointer.cast(),
                    size_bytes: node.size_bytes,
                    arena_id: node.arena_id,
                    backing: PhantomData,
                });
            }
            link = &mut node.next;
        }
        None
    }
}

impl Default for HeapSlotList<'_> {
    fn default() -> Self {
        Self::new()
    }
}
