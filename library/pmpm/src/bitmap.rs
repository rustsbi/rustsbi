//! PMP slot management bitmap.
//!
//! PMP slot bitmap used for allocating/freeing a PMP slot.

// PMP management bitmap, for alloc/free PMP slot in Penglai/Keystone. A bitmap
// can describe a segment of consecutive PMP entries.
use super::{MAX_PMP_ENTRY_COUNT, PmpSlot};
use core::sync::atomic::Ordering;
// RV32 has no 64-bit atomics. The slot word is 16 bits, so a 32-bit atomic holds it.
#[cfg(target_arch = "riscv32")]
use core::sync::atomic::AtomicU32 as SlotWord;
#[cfg(not(target_arch = "riscv32"))]
use core::sync::atomic::AtomicU64 as SlotWord;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum PmpError {
    IndexOutOfRange,
    NoFreeSlot,
}

#[derive(Debug)]
pub struct PMPSlotAllocator {
    pmp_slots: SlotWord,
    /// PMP slots that can be used.
    alloc_mask: u64,
    /// PMP slots that current allocator managed, including reserved ones.
    manage_mask: u64,
}

impl PMPSlotAllocator {
    /// Create new PMP bitmap.
    pub const fn new(alloc_mask: u64, manage_mask: u64) -> Self {
        // Only the low MAX_PMP_ENTRY_COUNT bits name real slots. Higher bits must not be
        // allocable, or a slot could be marked allocated without a valid index.
        const SLOTS: u64 = (1u64 << MAX_PMP_ENTRY_COUNT) - 1;
        PMPSlotAllocator {
            // use alloc mask to init PMP bitmap
            pmp_slots: SlotWord::new(0),
            alloc_mask: alloc_mask & SLOTS,
            manage_mask: manage_mask & SLOTS,
        }
    }

    fn load_bits(&self) -> u64 {
        self.pmp_slots.load(Ordering::Acquire) as u64
    }

    fn cas_bits(&self, cur: u64, new: u64) -> Result<(), ()> {
        self.pmp_slots
            .compare_exchange(cur as _, new as _, Ordering::Release, Ordering::Relaxed)
            .map(|_| ())
            .map_err(|_| ())
    }

    /// Alloc first free PMP slot in bitmap and update bitmap.
    pub fn alloc(&mut self) -> Result<PmpSlot, PmpError> {
        loop {
            let cur_slots = self.load_bits();
            // check avaliable slot
            let aval_slots = !cur_slots & self.alloc_mask;
            if aval_slots == 0 {
                return Err(PmpError::NoFreeSlot);
            }

            // alloc bit
            let pmp_idx = aval_slots.trailing_zeros();
            let new_bitmap = cur_slots | (1u64 << pmp_idx);
            // set new alloc bit
            match self.cas_bits(cur_slots, new_bitmap) {
                // alloc_mask only has bits below MAX_PMP_ENTRY_COUNT, so this is a valid slot.
                Ok(_) => return Ok(PmpSlot::new(pmp_idx).ok_or(PmpError::IndexOutOfRange)?),
                Err(_) => continue,
            }
        }
    }

    /// Free an allocated slot. Fails if the slot cannot be allocated or is not allocated.
    pub fn free(&mut self, slot: PmpSlot) -> Result<(), PmpError> {
        let free_slot = 1u64 << slot.index();
        loop {
            let cur_slots = self.load_bits();
            if (cur_slots & free_slot & self.alloc_mask) == 0 {
                return Err(PmpError::IndexOutOfRange);
            }

            let new_bitmap = cur_slots & !free_slot;

            match self.cas_bits(cur_slots, new_bitmap) {
                Ok(_) => return Ok(()),
                Err(_) => continue,
            }
        }
    }

    /// Mark `slot` as taken so `alloc` never returns it. Used for entries the hardware has
    /// locked, which cannot be rewritten until reset. Works for any index, allocable or not.
    /// `free` on a reserved slot succeeds, so only call this for slots that will never be freed.
    pub fn reserve(&mut self, slot: PmpSlot) {
        let bit = 1u64 << slot.index();
        loop {
            let cur_slots = self.load_bits();
            if cur_slots & bit != 0 {
                return;
            }
            if self.cas_bits(cur_slots, cur_slots | bit).is_ok() {
                return;
            }
        }
    }

    /// Whether `slot` is allocated. Fails if the slot is not allocable.
    pub fn is_alloc(&self, slot: PmpSlot) -> Result<bool, PmpError> {
        let check_slot = 1u64 << slot.index();
        if (check_slot & self.alloc_mask) == 0 {
            return Err(PmpError::IndexOutOfRange);
        }
        let cur_slots = self.load_bits();
        Ok((cur_slots & check_slot) != 0)
    }

    /// Whether `slot` is managed by this allocator, including reserved slots.
    pub fn is_managed(&self, slot: PmpSlot) -> bool {
        self.manage_mask & (1u64 << slot.index()) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MAX_PMP_ENTRY_COUNT;

    fn slot(idx: u32) -> PmpSlot {
        PmpSlot::new(idx).unwrap()
    }

    // --- Utility function to create masks ---
    fn create_mask(indices: &[u8]) -> u64 {
        let mut mask = 0u64;
        for &idx in indices {
            if idx < 64 {
                mask |= 1u64 << idx;
            }
        }
        mask
    }

    // --- Test: Initialization and Basic Allocation ---
    #[test]
    fn test_init_and_basic_alloc() {
        // Mask: only PMP indices 0, 1, 8, 9 are available
        let mask_indices = [0, 1, 8, 9];
        let alloc_mask = create_mask(&mask_indices);
        let mut allocator = PMPSlotAllocator::new(alloc_mask, alloc_mask);

        // Check initial state (should be 0)
        assert_eq!(allocator.pmp_slots.load(Ordering::Relaxed), 0);

        // 1. Allocate first slot (expected 0, the lowest set bit)
        let idx1 = allocator.alloc().unwrap();
        assert_eq!(idx1.index(), 0);
        assert_eq!(allocator.pmp_slots.load(Ordering::Relaxed), 0b1);

        // 2. Allocate second slot (expected 1)
        let idx2 = allocator.alloc().unwrap();
        assert_eq!(idx2.index(), 1);
        assert_eq!(allocator.pmp_slots.load(Ordering::Relaxed), 0b11);

        // 3. Allocate third slot (expected 8, skipping 2-7)
        let idx3 = allocator.alloc().unwrap();
        assert_eq!(idx3.index(), 8);
        assert_eq!(allocator.pmp_slots.load(Ordering::Relaxed), 0b100000011);

        // 4. Allocate fourth slot (expected 9)
        let idx4 = allocator.alloc().unwrap();
        assert_eq!(idx4.index(), 9);
        assert_eq!(allocator.pmp_slots.load(Ordering::Relaxed), 0b1100000011);
    }

    // --- Test: Range Limit and No Free Slot ---
    #[test]
    fn test_no_free_slot_and_limit() {
        // Mask: only PMP index 15 is available
        let alloc_mask = 1u64 << 15;
        let mut allocator = PMPSlotAllocator::new(alloc_mask, alloc_mask);

        // 1. Allocate the only slot
        let idx = allocator.alloc().unwrap();
        assert_eq!(idx.index(), 15);

        // 2. Try to allocate again
        assert_eq!(allocator.alloc(), Err(PmpError::NoFreeSlot));

        // 3. Free the slot
        allocator.free(slot(15)).unwrap();

        // 4. Allocate again (should succeed)
        assert_eq!(allocator.alloc().unwrap().index(), 15);
    }

    // --- Test: Free Logic and Error Handling ---
    #[test]
    fn test_free_errors() {
        // Mask: PMP indices 4, 5, 6 are available
        let alloc_mask = create_mask(&[4, 5, 6]);
        let mut allocator = PMPSlotAllocator::new(alloc_mask, alloc_mask);

        // Allocate slot 4
        allocator.alloc().unwrap();

        // 1. Error: slot not in the alloc mask (0 is not allocable)
        assert_eq!(allocator.free(slot(0)), Err(PmpError::IndexOutOfRange));

        // 2. Out-of-range indices cannot be represented as PmpSlot at all
        assert!(PmpSlot::new(MAX_PMP_ENTRY_COUNT).is_none());

        // 3. Error: slot not allocated (5 is allocable, but free)
        assert_eq!(allocator.free(slot(5)), Err(PmpError::IndexOutOfRange));

        // 4. Success: Free allocated slot 4
        assert_eq!(allocator.free(slot(4)), Ok(()));

        // 5. Error: slot not allocated (4 is now free)
        assert_eq!(allocator.free(slot(4)), Err(PmpError::IndexOutOfRange));
    }

    // --- Test: reserved slots are managed, and never allocated ---
    #[test]
    fn test_managed_reserved_not_allocated() {
        // Alloc mask excludes slots 0, 1, 15; manage mask includes them.
        let alloc_mask = create_mask(&[2, 3]);
        let manage_mask = create_mask(&[0, 1, 2, 3, 15]);
        let mut allocator = PMPSlotAllocator::new(alloc_mask, manage_mask);
        assert!(allocator.is_managed(slot(0)));
        assert!(allocator.is_managed(slot(15)));
        assert!(!allocator.is_managed(slot(4)));
        assert_eq!(allocator.alloc().unwrap().index(), 2);
        assert_eq!(allocator.alloc().unwrap().index(), 3);
        assert_eq!(allocator.alloc(), Err(PmpError::NoFreeSlot));
    }

    // --- Test: locked (reserved) slots are never handed out ---
    #[test]
    fn test_reserved_slot_not_allocated() {
        let alloc_mask = create_mask(&[2, 3, 4]);
        let mut allocator = PMPSlotAllocator::new(alloc_mask, alloc_mask);
        allocator.reserve(slot(3));
        // Reserving twice is harmless.
        allocator.reserve(slot(3));
        assert_eq!(allocator.alloc().unwrap().index(), 2);
        assert_eq!(allocator.alloc().unwrap().index(), 4);
        assert_eq!(allocator.alloc(), Err(PmpError::NoFreeSlot));
    }

    // --- Test: is_alloc Check ---
    const TEST_BIT1: u32 = MAX_PMP_ENTRY_COUNT - 1;
    const TEST_BIT2: u32 = MAX_PMP_ENTRY_COUNT - 2;
    const TEST_BIT3: u32 = MAX_PMP_ENTRY_COUNT - 3;
    #[test]
    fn test_is_alloc() {
        let alloc_mask = create_mask(&[TEST_BIT1 as u8, TEST_BIT2 as u8, TEST_BIT3 as u8]);
        let mut allocator = PMPSlotAllocator::new(alloc_mask, alloc_mask);

        // 1. In range, but free
        assert_eq!(allocator.is_alloc(slot(TEST_BIT1)), Ok(false));

        // 2. Not allocable
        assert_eq!(
            allocator.is_alloc(slot(TEST_BIT3 - 1)),
            Err(PmpError::IndexOutOfRange)
        );
        assert!(PmpSlot::new(MAX_PMP_ENTRY_COUNT).is_none());

        // 3. Allocate
        allocator.alloc().unwrap();
        allocator.alloc().unwrap();

        // 4. In range, and allocated
        assert_eq!(allocator.is_alloc(slot(TEST_BIT3)), Ok(true));
        assert_eq!(allocator.is_alloc(slot(TEST_BIT2)), Ok(true));
        assert_eq!(allocator.is_alloc(slot(TEST_BIT1)), Ok(false)); // Still free
    }
}
