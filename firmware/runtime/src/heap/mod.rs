//! Owned heap memory and the interface for a policy-defined global allocator.
//!
//! Runtime owns the linker heap and the Rust allocation boundary. Firmware
//! policy decides how to place, split, and reuse slots within that heap.

use core::alloc::Layout;

mod dispatch;
mod firmware;
mod memory;

pub use firmware::{register_global_heap_allocator, take_heap};
pub use memory::{HeapSlot, HeapSlotList};

/// An allocation request that cannot be satisfied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeapAllocError;

/// The global heap allocator has already been registered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeapAlreadyRegistered;

/// A stable mapping from Rust requests to the size of policy-owned slots.
///
/// Runtime keeps a copy at registration. Unlike a policy callback, this map
/// cannot change between allocation and deallocation. Alignment remains part
/// of each request and must be honored independently of the slot size.
#[derive(Clone, Copy, Debug)]
pub struct SlotSizeMap {
    kind: SlotSizeKind,
}

#[derive(Clone, Copy, Debug)]
enum SlotSizeKind {
    Rounded(usize),
    PowerOfTwo(usize),
}

impl SlotSizeMap {
    /// Rounds every nonzero request up to a power-of-two byte granule.
    pub const fn rounded(granule: usize) -> Self {
        assert!(granule.is_power_of_two());
        Self {
            kind: SlotSizeKind::Rounded(granule),
        }
    }

    /// Uses the next power-of-two size, at least the minimum and alignment.
    pub const fn power_of_two(minimum: usize) -> Self {
        assert!(minimum.is_power_of_two());
        Self {
            kind: SlotSizeKind::PowerOfTwo(minimum),
        }
    }

    fn slot_layout(self, request: Layout) -> Option<Layout> {
        let size_bytes = request.size().max(1);
        let size_bytes = match self.kind {
            SlotSizeKind::Rounded(granule) => size_bytes
                .checked_add(granule - 1)
                .map(|size| size & !(granule - 1))?,
            SlotSizeKind::PowerOfTwo(minimum) => size_bytes
                .max(minimum)
                .max(request.align())
                .checked_next_power_of_two()?,
        };
        Layout::from_size_align(size_bytes, request.align()).ok()
    }
}

/// A policy-defined allocator of owned heap slots.
///
/// Requests carry the mapped slot size and the Rust request's alignment. The
/// returned slot must have exactly that size, be properly aligned, and be
/// derived from the linker heap supplied by `take_heap`. Runtime checks these
/// conditions before exposing its address as a Rust allocation. On deallocation
/// it recreates the same slot from the fixed size map and returns ownership.
///
/// Implementations must not use the global allocator or unwind. Firmware aborts
/// on panic. Implementations provide their own synchronization between harts.
pub trait GlobalHeapAllocator: Sync {
    /// Allocates a slot with exactly `slot_layout.size()` bytes.
    fn alloc(&self, slot_layout: Layout) -> Result<HeapSlot<'static>, HeapAllocError>;

    /// Takes ownership of a slot being returned to the allocator.
    fn dealloc(&self, slot: HeapSlot<'static>);
}
