//! Secure regions backed by PMP, with one temporary allocation grant per hart.

use super::*;
use crate::protector::{SecMemProtectorByPMP, UpdatePmp};
use alloc::vec::Vec;
use core::ops::Range;

const MONITOR_SLOT: u32 = 0;
const TEMP_SLOT: u32 = 1;
const DEFAULT_SLOT: u32 = MAX_PMP_ENTRY_COUNT - 1;
const MANAGED: u64 = (1 << MAX_PMP_ENTRY_COUNT) - 1;
const ALLOCATABLE: u64 = MANAGED & !((1 << MONITOR_SLOT) | (1 << TEMP_SLOT) | (1 << DEFAULT_SLOT));

enum Monitor {
    Uninitialized,
    Protected(Range<usize>),
    Ready(Range<usize>),
}

pub struct UniSecMemManager<const ORDER: usize, AR, AA>
where
    AR: SecMemAllocator<ORDER>,
    AA: SecMemAllocator<ORDER>,
{
    cur_idx: usize,
    protector: SecMemProtectorByPMP,
    monitor: Monitor,
    alloc_regions: Vec<SecMemRegion<ORDER, AR, AA>>,
}

impl<const ORDER: usize, AR, AA> UniSecMemManager<ORDER, AR, AA>
where
    AR: SecMemAllocator<ORDER>,
    AA: SecMemAllocator<ORDER>,
{
    fn release_region(&mut self, index: usize) -> Option<(usize, usize)> {
        let region = &mut self.alloc_regions[index];
        if !region.granted_allocations.is_empty() {
            return None;
        }
        if !matches!(region.allocator, SecMemAllocatorWrapper::None) {
            region.allocator = SecMemAllocatorWrapper::None;
            region.live_allocations = 0;
            // SAFETY: extend reserves exclusive writable memory. No hart has a live grant.
            unsafe { core::ptr::write_bytes(region.addr as *mut u8, 0, region.len) };
        }
        if !self.protector.disable(region.slot) {
            return None;
        }
        assert!(self.protector.free(region.slot));
        let region = self.alloc_regions.swap_remove(index);
        Some((region.addr, region.len))
    }
}

impl<const ORDER: usize, AR, AA> SecMemManager<ORDER, AR, AA> for UniSecMemManager<ORDER, AR, AA>
where
    AR: SecMemAllocator<ORDER>,
    AA: SecMemAllocator<ORDER>,
{
    unsafe fn new(update_local: UpdatePmp, update_all: UpdatePmp) -> Self {
        Self {
            cur_idx: 0,
            // SAFETY: The caller reserves the managed entries and supplies callbacks
            // that synchronize successful updates and preserve protection on failure.
            protector: unsafe {
                SecMemProtectorByPMP::new(ALLOCATABLE, MANAGED, update_local, update_all)
            },
            monitor: Monitor::Uninitialized,
            alloc_regions: Vec::with_capacity(ALLOCATABLE.count_ones() as usize),
        }
    }
    unsafe fn init(&mut self, sm_addr: usize, sm_len: usize) -> bool {
        let Some(end) = sm_addr.checked_add(sm_len) else {
            return false;
        };
        let range = sm_addr..end;
        match &self.monitor {
            Monitor::Uninitialized => {
                if !self.protector.is_protectable(sm_addr, sm_len)
                    || !self
                        .protector
                        .retrive_access_all(sm_addr, sm_len, MONITOR_SLOT)
                {
                    return false;
                }
                self.monitor = Monitor::Protected(range.clone());
            }
            Monitor::Protected(previous) if *previous == range => {}
            _ => return false,
        }
        if !self.protector.grant_access_all(0, usize::MAX, DEFAULT_SLOT) {
            return false;
        }
        self.monitor = Monitor::Ready(range);
        true
    }

    fn deinit(&mut self, cur_regions: &mut [(usize, usize); MAX_PMP_ENTRY_COUNT as usize]) -> u32 {
        let mut count = 0;
        while let Some(index) = self.alloc_regions.len().checked_sub(1) {
            let Some(region) = self.release_region(index) else {
                break;
            };
            cur_regions[count] = region;
            count += 1;
        }
        count as u32
    }

    unsafe fn extend(&mut self, addr: usize, len: usize) -> bool {
        let (Monitor::Ready(monitor), Some(end), Some(id)) = (
            &self.monitor,
            addr.checked_add(len),
            self.cur_idx.checked_add(1),
        ) else {
            return false;
        };
        if addr == 0
            || !self.protector.is_protectable(addr, len)
            || (addr < monitor.end && monitor.start < end)
            || self
                .alloc_regions
                .iter()
                .any(|r| r.is_mem_overlap(addr, len))
        {
            return false;
        }
        let Some(slot) = self.protector.alloc() else {
            return false;
        };
        if !self.protector.retrive_access_all(addr, len, slot) {
            assert!(self.protector.free(slot));
            return false;
        }
        self.cur_idx = id;
        self.alloc_regions
            .push(SecMemRegion::new(addr, len, id, slot));
        true
    }

    fn reclaim(&mut self) -> Option<(usize, usize)> {
        let index = self
            .alloc_regions
            .iter()
            .position(|region| region.live_allocations == 0)?;
        self.release_region(index)
    }

    fn alloc_em(&mut self, len: usize, em_type: SecMemType) -> Option<(usize, usize, usize)> {
        fn allocate<const ORDER: usize>(
            allocator: &mut impl SecMemAllocator<ORDER>,
            maximum: usize,
            minimum: usize,
        ) -> Option<(NonNull<u8>, usize)> {
            let mut size = 1usize << maximum.checked_ilog2()?;
            while size >= minimum {
                if let Ok(layout) = Layout::from_size_align(size, size)
                    && let Ok(ptr) = allocator.alloc(layout)
                {
                    return Some((ptr, size));
                }
                size /= 2;
            }
            None
        }

        if len == 0 {
            return None;
        }
        let requested = len.max(8).checked_next_power_of_two()?;

        for region in self.alloc_regions.iter_mut() {
            if region.len < requested {
                continue;
            }

            let fresh = matches!(region.allocator, SecMemAllocatorWrapper::General);
            if fresh {
                region.allocator = match em_type {
                    SecMemType::Runtime => {
                        let mut alloc = AR::new();
                        alloc.init(region.addr, region.len);
                        SecMemAllocatorWrapper::Runtime(alloc)
                    }
                    SecMemType::Application => {
                        let mut alloc = AA::new();
                        alloc.init(region.addr, region.len);
                        SecMemAllocatorWrapper::Application(alloc)
                    }
                    _ => return None,
                };
            }
            let result = match (&mut region.allocator, em_type) {
                (SecMemAllocatorWrapper::Runtime(alloc), SecMemType::Runtime) => {
                    let size = alloc.available().min(region.len);
                    allocate(alloc, size, requested)
                }
                (SecMemAllocatorWrapper::Application(alloc), SecMemType::Application) => {
                    allocate(alloc, requested, requested)
                }
                _ => continue,
            };
            if let Some((ptr, size)) = result {
                let ptr_addr = ptr.as_ptr() as usize;
                if ptr_addr.is_multiple_of(size) && region.is_mem_contained(ptr_addr, size) {
                    region.live_allocations += 1;
                    return Some((ptr_addr, size, region.id));
                }
            }
            if fresh {
                region.allocator = SecMemAllocatorWrapper::General;
            }
        }
        None
    }

    fn free_em(&mut self, addr: usize, len: usize) -> Option<usize> {
        let free_layout = Layout::from_size_align(len, len).ok()?;
        let free_ptr = NonNull::new(addr as *mut u8)?;

        let free_region = self
            .alloc_regions
            .iter_mut()
            .find(|region| region.is_mem_contained(addr, len))?;
        if free_region.granted_allocations.contains(&addr) {
            return None;
        }
        fn release<const ORDER: usize>(
            allocator: &mut impl SecMemAllocator<ORDER>,
            ptr: NonNull<u8>,
            layout: Layout,
        ) -> bool {
            if !allocator.owns(ptr.as_ptr() as usize, layout.size()) {
                return false;
            }
            // SAFETY: The registered range is writable, owns confirms the whole
            // allocation, and free_em checked that every hart has revoked its grant.
            unsafe { core::ptr::write_bytes(ptr.as_ptr(), 0, layout.size()) };
            // The allocator may reuse the block for its free list here.
            allocator.free(ptr, layout)
        }
        let released = match &mut free_region.allocator {
            SecMemAllocatorWrapper::Application(alloc) => release(alloc, free_ptr, free_layout),
            SecMemAllocatorWrapper::Runtime(alloc) => release(alloc, free_ptr, free_layout),
            _ => false,
        };
        if !released {
            return None;
        }
        free_region.live_allocations -= 1;
        if free_region.live_allocations == 0 {
            free_region.allocator = SecMemAllocatorWrapper::General;
        }
        Some(free_region.id)
    }

    fn grant_access(&mut self, addr: usize, len: usize, region_id: usize) -> bool {
        let Some(region) = self
            .alloc_regions
            .iter_mut()
            .find(|region| region.id == region_id && region.is_mem_contained(addr, len))
        else {
            return false;
        };
        let live = match &region.allocator {
            SecMemAllocatorWrapper::Application(alloc) => alloc.owns(addr, len),
            SecMemAllocatorWrapper::Runtime(alloc) => alloc.owns(addr, len),
            _ => false,
        };
        if !live
            || region.granted_allocations.try_reserve(1).is_err()
            || !self.protector.grant_access(addr, len, TEMP_SLOT)
        {
            return false;
        }
        region.granted_allocations.push(addr);
        true
    }
    fn retrive_access(&mut self, addr: usize, len: usize, region_id: usize) -> bool {
        let Some(region) = self
            .alloc_regions
            .iter_mut()
            .find(|region| region.id == region_id && region.is_mem_contained(addr, len))
        else {
            return false;
        };
        let Some(index) = region.granted_allocations.iter().position(|&a| a == addr) else {
            return false;
        };
        if !self.protector.retrive_access(addr, len, TEMP_SLOT) {
            return false;
        }
        region.granted_allocations.swap_remove(index);
        true
    }
}
