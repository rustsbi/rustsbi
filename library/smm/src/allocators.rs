//! Example allocators for secure-memory regions.
//!
//! [`AppAlloc`] uses first-fit placement in a shared region with a fixed limit
//! on live allocations. [`RTAlloc`] hands out its region to one enclave at a time.

use self::region::RegionAllocator;
use super::SecMemAllocator;
use core::alloc::Layout;
use core::ptr::NonNull;

mod region;

/// First-fit allocator for a shared secure-memory region.
///
/// `MAX_LIVE_ALLOCATIONS` bounds simultaneously live allocations. Reaching
/// that limit returns an allocation error and reports zero availability even
/// if the region has free bytes.
/// Records are stored inline; allocation and free do not use the global heap.
pub struct AppAlloc<const ORDER: usize, const MAX_LIVE_ALLOCATIONS: usize> {
    range_allocator: RegionAllocator<MAX_LIVE_ALLOCATIONS>,
}

pub struct RTAlloc<const ORDER: usize> {
    start_addr: usize,
    available_bytes: usize,
    total_bytes: usize,
}

pub struct NoneAlloc<const ORDER: usize> {}

impl<const ORDER: usize> SecMemAllocator<ORDER> for NoneAlloc<ORDER> {
    fn new() -> Self {
        Self {}
    }
}

impl<const ORDER: usize, const MAX_LIVE_ALLOCATIONS: usize> SecMemAllocator<ORDER>
    for AppAlloc<ORDER, MAX_LIVE_ALLOCATIONS>
{
    fn new() -> Self {
        Self {
            range_allocator: RegionAllocator::new(),
        }
    }
    fn init(&mut self, start_addr: usize, size_bytes: usize) {
        let end_addr = start_addr
            .checked_add(size_bytes)
            .expect("[AppAlloc] memory range overflow");
        self.range_allocator
            .initialize(start_addr, end_addr)
            .expect("[AppAlloc] invalid memory range");
    }
    fn alloc(&mut self, layout: Layout) -> Result<NonNull<u8>, ()> {
        let start_addr = self.range_allocator.alloc(layout)?;
        Ok(NonNull::new(start_addr as *mut u8).expect("region allocator excludes address zero"))
    }
    fn free(&mut self, ptr: NonNull<u8>, layout: Layout) {
        self.range_allocator
            .free(ptr.as_ptr() as usize, layout)
            .expect("[AppAlloc] deallocating an invalid allocation");
    }
    fn available(&self) -> usize {
        self.range_allocator.available_bytes()
    }
    fn total(&self) -> usize {
        self.range_allocator.total_bytes()
    }
}

/// Memory allocation style of Keystone. Enclaves in Keystone exclusively occupy
/// the entire region, thus the region is merely marked and no further allocation
/// is performed within it.
impl<const ORDER: usize> SecMemAllocator<ORDER> for RTAlloc<ORDER> {
    fn new() -> Self {
        Self {
            start_addr: 0,
            available_bytes: 0,
            total_bytes: 0,
        }
    }
    fn init(&mut self, start_addr: usize, size_bytes: usize) {
        self.start_addr = start_addr;
        self.available_bytes = size_bytes;
        self.total_bytes = size_bytes;
    }
    fn alloc(&mut self, layout: Layout) -> Result<NonNull<u8>, ()> {
        if self.available_bytes < layout.size() {
            return Err(());
        }
        self.available_bytes = 0;
        NonNull::new(self.start_addr as *mut u8).ok_or(())
    }
    fn free(&mut self, ptr: NonNull<u8>, layout: Layout) {
        if ptr.as_ptr() as usize != self.start_addr {
            panic!("[RTAlloc] Deallocating foreign or incorrect pointer");
        }
        self.available_bytes = layout.size();
    }
    fn available(&self) -> usize {
        self.available_bytes
    }
    fn total(&self) -> usize {
        self.total_bytes
    }
}

#[allow(static_mut_refs)]
#[cfg(test)]
mod stress_tests {
    use super::*;
    use alloc::vec::Vec;
    use core::alloc::Layout;
    use rand::rngs::StdRng;
    use rand::{Rng, SeedableRng};

    const TEST_EXPONENT: usize = 28;
    const TEST_MEM_SIZE: usize = 1 << TEST_EXPONENT; // 256MB
    #[repr(C, align(4096))]
    struct TestMemory([u8; TEST_MEM_SIZE]);
    static mut FAKE_HARDWARE_MEM: TestMemory = TestMemory([0; TEST_MEM_SIZE]);
    #[test]
    fn stress_test_app_alloc_power_of_two_silent() {
        const MEM_SIZE: usize = TEST_MEM_SIZE;
        let raw_mem = unsafe { FAKE_HARDWARE_MEM.0.as_mut_ptr() };

        let mut allocator = AppAlloc::<TEST_EXPONENT, 64>::new();
        allocator.init(raw_mem as usize, MEM_SIZE);

        let initial_available = allocator.available();
        let mut rng = StdRng::seed_from_u64(42);
        let mut allocations = Vec::with_capacity(128);
        let mut size_hit_map = [0u32; 26];

        for _ in 0..5000 {
            if !allocations.is_empty() && (rng.gen_bool(0.3) || allocations.len() > 30) {
                let index = rng.gen_range(0..allocations.len());
                let (ptr, layout) = allocations.remove(index);
                allocator.free(ptr, layout);
            } else {
                let exponent = rng.gen_range(12..26);
                let size = 1 << exponent;
                let layout = Layout::from_size_align(size, size).unwrap();

                if let Ok(ptr) = allocator.alloc(layout) {
                    size_hit_map[exponent as usize] += 1;
                    allocations.push((ptr, layout));
                }
            }
        }

        for (ptr, layout) in allocations {
            allocator.free(ptr, layout);
        }

        for exp in 12..26 {
            assert!(
                size_hit_map[exp] > 0,
                "Size 2^{} was never successfully allocated",
                exp
            );
        }

        assert_eq!(allocator.available(), initial_available);
    }

    #[test]
    fn app_alloc_reuses_its_explicit_record_capacity() {
        let mut backing = [0u8; 64];
        let mut allocator = AppAlloc::<6, 1>::new();
        allocator.init(backing.as_mut_ptr() as usize, backing.len());
        let layout = Layout::from_size_align(8, 1).unwrap();
        let first = allocator.alloc(layout).unwrap();
        assert_eq!(allocator.available(), 0);
        assert!(allocator.alloc(layout).is_err());
        allocator.free(first, layout);
        assert_eq!(allocator.available(), backing.len());
        assert_eq!(allocator.alloc(layout), Ok(first));
    }
    #[test]
    fn stress_test_rt_alloc() {
        let mut allocator = RTAlloc::<TEST_EXPONENT>::new();
        let base_addr: usize = unsafe { FAKE_HARDWARE_MEM.0.as_ptr() as usize };
        let layout = Layout::from_size_align(TEST_MEM_SIZE, TEST_MEM_SIZE).unwrap();

        allocator.init(base_addr, TEST_MEM_SIZE);
        let ptr = allocator.alloc(layout).unwrap();
        assert_eq!(allocator.available(), 0);

        allocator.free(ptr, layout);
        assert_eq!(allocator.available(), TEST_MEM_SIZE);
    }
}
