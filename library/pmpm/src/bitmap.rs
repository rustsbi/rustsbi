//! Allocation of the PMP entries managed by this library.

use super::MAX_PMP_ENTRY_COUNT;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum PmpError {
    IndexOutOfRange,
    NoFreeSlot,
}

#[derive(Debug)]
pub struct PMPSlotAllocator {
    pmp_slots: u64,
    /// PMP slots that can be used.
    alloc_mask: u64,
    /// PMP slots that current allocator managed, including reserved ones.
    manage_mask: u64,
}

impl PMPSlotAllocator {
    /// Create new PMP bitmap.
    pub const fn new(alloc_mask: u64, manage_mask: u64) -> Self {
        let manage_mask = manage_mask & ((1 << MAX_PMP_ENTRY_COUNT) - 1);
        Self {
            pmp_slots: 0,
            alloc_mask: alloc_mask & manage_mask,
            manage_mask,
        }
    }

    /// Alloc first free PMP slot in bitmap and update bitmap.
    pub fn alloc(&mut self) -> Result<u32, PmpError> {
        let available = self.alloc_mask & !self.pmp_slots;
        if available == 0 {
            return Err(PmpError::NoFreeSlot);
        }
        let index = available.trailing_zeros();
        self.pmp_slots |= 1 << index;
        Ok(index)
    }

    pub fn free(&mut self, idx: u32) -> Result<(), PmpError> {
        if !self.is_alloc(idx)? {
            return Err(PmpError::IndexOutOfRange);
        }
        self.pmp_slots &= !(1 << idx);
        Ok(())
    }

    pub fn is_alloc(&self, idx: u32) -> Result<bool, PmpError> {
        if idx >= MAX_PMP_ENTRY_COUNT || self.alloc_mask & (1 << idx) == 0 {
            return Err(PmpError::IndexOutOfRange);
        }
        Ok(self.pmp_slots & (1 << idx) != 0)
    }

    pub fn is_managed(&self, idx: u32) -> bool {
        idx < MAX_PMP_ENTRY_COUNT && self.manage_mask & (1 << idx) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        assert_eq!(allocator.pmp_slots, 0);

        // 1. Allocate first slot (expected 0, the lowest set bit)
        let idx1 = allocator.alloc().unwrap();
        assert_eq!(idx1, 0);
        assert_eq!(allocator.pmp_slots, 0b1);

        // 2. Allocate second slot (expected 1)
        let idx2 = allocator.alloc().unwrap();
        assert_eq!(idx2, 1);
        assert_eq!(allocator.pmp_slots, 0b11);

        // 3. Allocate third slot (expected 8, skipping 2-7)
        let idx3 = allocator.alloc().unwrap();
        assert_eq!(idx3, 8);
        assert_eq!(allocator.pmp_slots, 0b100000011);

        // 4. Allocate fourth slot (expected 9)
        let idx4 = allocator.alloc().unwrap();
        assert_eq!(idx4, 9);
        assert_eq!(allocator.pmp_slots, 0b1100000011);
    }

    // --- Test: Range Limit and No Free Slot ---
    #[test]
    fn test_no_free_slot_and_limit() {
        // Mask: only PMP index 15 is available
        let alloc_mask = 1u64 << 15;
        let mut allocator = PMPSlotAllocator::new(alloc_mask, alloc_mask);

        // 1. Allocate the only slot
        let idx = allocator.alloc().unwrap();
        assert_eq!(idx, 15);

        // 2. Try to allocate again
        assert_eq!(allocator.alloc(), Err(PmpError::NoFreeSlot));

        // 3. Free the slot
        allocator.free(15).unwrap();

        // 4. Allocate again (should succeed)
        assert_eq!(allocator.alloc().unwrap(), 15);
    }

    // --- Test: Free Logic and Error Handling ---
    #[test]
    fn test_free_errors() {
        // Mask: PMP indices 4, 5, 6 are available
        let alloc_mask = create_mask(&[4, 5, 6]);
        let mut allocator = PMPSlotAllocator::new(alloc_mask, alloc_mask);

        // Allocate slot 4
        allocator.alloc().unwrap();

        // 1. Error: Index not in range (idx 0 is not in the mask)
        assert_eq!(allocator.free(0), Err(PmpError::IndexOutOfRange));

        // 2. Error: Index not in range (idx 63 is too high)
        assert_eq!(allocator.free(63), Err(PmpError::IndexOutOfRange));

        // 3. Error: Index not allocated (idx 5 is in range, but free)
        assert_eq!(allocator.free(5), Err(PmpError::IndexOutOfRange));

        // 4. Success: Free allocated slot 4
        assert_eq!(allocator.free(4), Ok(()));

        // 5. Error: Index not allocated (idx 4 is now free)
        assert_eq!(allocator.free(4), Err(PmpError::IndexOutOfRange));
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
        assert_eq!(allocator.is_alloc(TEST_BIT1), Ok(false));

        // 2. Out of range
        assert_eq!(
            allocator.is_alloc(TEST_BIT1 + 1),
            Err(PmpError::IndexOutOfRange)
        );
        assert_eq!(
            allocator.is_alloc(TEST_BIT3 - 1),
            Err(PmpError::IndexOutOfRange)
        );
        assert_eq!(
            allocator.is_alloc(MAX_PMP_ENTRY_COUNT),
            Err(PmpError::IndexOutOfRange)
        );

        // 3. Allocate
        allocator.alloc().unwrap();
        allocator.alloc().unwrap();

        // 4. In range, and allocated
        assert_eq!(allocator.is_alloc(TEST_BIT3), Ok(true));
        assert_eq!(allocator.is_alloc(TEST_BIT2), Ok(true));
        assert_eq!(allocator.is_alloc(TEST_BIT1), Ok(false)); // Still free
    }
}
