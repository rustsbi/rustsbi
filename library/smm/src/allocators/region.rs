//! Address accounting for shared secure-memory regions.
//!
//! Every live allocation has an inline record. The caller chooses the record
//! capacity, so allocating and freeing secure memory never use the global heap.

use core::{alloc::Layout, ops::Range};

pub(super) struct RegionAllocator<const MAX_LIVE_ALLOCATIONS: usize> {
    region: Range<usize>,
    allocations: [Range<usize>; MAX_LIVE_ALLOCATIONS],
    allocation_count: usize,
}

impl<const MAX_LIVE_ALLOCATIONS: usize> RegionAllocator<MAX_LIVE_ALLOCATIONS> {
    pub(super) fn new() -> Self {
        Self {
            region: 0..0,
            allocations: [const { 0..0 }; MAX_LIVE_ALLOCATIONS],
            allocation_count: 0,
        }
    }

    pub(super) fn initialize(&mut self, start_addr: usize, end_addr: usize) -> Result<(), ()> {
        if start_addr == 0 || start_addr >= end_addr || self.allocation_count != 0 {
            return Err(());
        }
        self.region = start_addr..end_addr;
        Ok(())
    }

    pub(super) fn alloc(&mut self, layout: Layout) -> Result<usize, ()> {
        if self.allocation_count == MAX_LIVE_ALLOCATIONS {
            return Err(());
        }
        let size_bytes = layout.size().max(1);
        let mut gap_start = self.region.start;
        for index in 0..=self.allocation_count {
            let gap_end = if index < self.allocation_count {
                self.allocations[index].start
            } else {
                self.region.end
            };
            let start_addr =
                gap_start.checked_add(layout.align() - 1).ok_or(())? & !(layout.align() - 1);
            let end_addr = start_addr.checked_add(size_bytes).ok_or(())?;
            if end_addr <= gap_end {
                for record in (index..self.allocation_count).rev() {
                    self.allocations[record + 1] = self.allocations[record].clone();
                }
                self.allocations[index] = start_addr..end_addr;
                self.allocation_count += 1;
                return Ok(start_addr);
            }
            if index < self.allocation_count {
                let range = &self.allocations[index];
                gap_start = range.end;
            }
        }
        Err(())
    }

    pub(super) fn free(&mut self, start_addr: usize, layout: Layout) -> Result<(), ()> {
        let end_addr = start_addr.checked_add(layout.size().max(1)).ok_or(())?;
        let index = self.allocations[..self.allocation_count]
            .binary_search_by_key(&start_addr, |range| range.start)
            .map_err(|_| ())?;
        if self.allocations[index].end != end_addr {
            return Err(());
        }
        for record in index..self.allocation_count - 1 {
            self.allocations[record] = self.allocations[record + 1].clone();
        }
        self.allocation_count -= 1;
        self.allocations[self.allocation_count] = 0..0;
        Ok(())
    }

    pub(super) fn total_bytes(&self) -> usize {
        self.region.end - self.region.start
    }

    pub(super) fn allocated_bytes(&self) -> usize {
        self.allocations[..self.allocation_count]
            .iter()
            .map(|range| range.end - range.start)
            .sum()
    }

    pub(super) fn available_bytes(&self) -> usize {
        if self.allocation_count == MAX_LIVE_ALLOCATIONS {
            return 0;
        }
        self.total_bytes() - self.allocated_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_frees_preserve_live_allocations() {
        let mut allocator = RegionAllocator::<1>::new();
        allocator.initialize(0x1003, 0x2000).unwrap();
        let layout = Layout::from_size_align(32, 256).unwrap();
        let address = allocator.alloc(layout).unwrap();
        assert!(address.is_multiple_of(256));
        let available_bytes = allocator.available_bytes();
        assert_eq!(allocator.free(address, Layout::new::<u8>()), Err(()));
        assert_eq!(allocator.free(address + 1, layout), Err(()));
        assert_eq!(allocator.available_bytes(), available_bytes);
        allocator.free(address, layout).unwrap();
        assert_eq!(allocator.free(address, layout), Err(()));
    }
}
