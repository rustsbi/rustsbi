//! Secure Memory Region Allocators
//!
//! Concrete **SecMemAllocator** examples for diverse TEE memory paradigms.
//! **AppAlloc**: Shared-region management using **Buddy System** (Penglai-style).
//! **RTAlloc**: Exclusive-region occupation for **RT** enclaves (Keystone-style).
//! Ensures security via strict **2^n** alignment and full-state recovery tests.

use super::SecMemAllocator;
use buddy_system_allocator::Heap;
use core::alloc::Layout;
use core::ptr::NonNull;

pub struct AppAlloc<const ORDER: usize> {
    buddy: Heap<ORDER>,
}
pub struct RTAlloc<const ORDER: usize> {
    addr: usize,
    len: usize,
    total: usize,
}

pub struct NoneAlloc<const ORDER: usize> {}

impl<const ORDER: usize> SecMemAllocator<ORDER> for NoneAlloc<ORDER> {
    fn new() -> Self {
        Self {}
    }
}

/// Memory allocation style of Penglai. Penglai supports enclaves sharing a
/// secure memory region, which is allocated using the buddy algorithm.
impl<const ORDER: usize> SecMemAllocator<ORDER> for AppAlloc<ORDER> {
    fn new() -> Self {
        Self {
            buddy: Heap::<ORDER>::new(),
        }
    }
    fn init(&mut self, addr: usize, len: usize) {
        unsafe {
            self.buddy.init(addr as usize, len as usize);
        }
    }
    fn alloc(&mut self, layout: Layout) -> Result<NonNull<u8>, ()> {
        self.buddy.alloc(layout)
    }
    fn free(&mut self, ptr: NonNull<u8>, layout: Layout) {
        self.buddy.dealloc(ptr, layout);
    }
    fn available(&self) -> usize {
        self.buddy.stats_total_bytes() - self.buddy.stats_alloc_actual()
    }
    fn total(&self) -> usize {
        self.buddy.stats_total_bytes()
    }
}

/// Memory allocation style of Keystone. Enclaves in Keystone exclusively occupy
/// the entire region, thus the region is merely marked and no further allocation
/// is performed within it.
impl<const ORDER: usize> SecMemAllocator<ORDER> for RTAlloc<ORDER> {
    fn new() -> Self {
        Self {
            addr: 0,
            len: 0,
            total: 0,
        }
    }
    fn init(&mut self, addr: usize, len: usize) {
        self.addr = addr;
        self.len = len;
        self.total = len;
    }
    fn alloc(&mut self, layout: Layout) -> Result<NonNull<u8>, ()> {
        if (self.len as usize) < layout.size() {
            return Err(());
        }
        self.len = 0;
        Ok(NonNull::new(self.addr as *mut u8).ok_or(())?)
    }
    fn free(&mut self, ptr: NonNull<u8>, layout: Layout) {
        if ptr.as_ptr() as usize != self.addr {
            panic!("[RTAlloc] Deallocating foreign or incorrect pointer");
        }
        self.len = layout.size() as usize;
    }
    fn available(&self) -> usize {
        self.len as usize
    }
    fn total(&self) -> usize {
        self.total as usize
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

        let mut allocator = AppAlloc::<TEST_EXPONENT>::new();
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
    fn stress_test_rt_alloc() {
        let mut allocator = RTAlloc::<TEST_EXPONENT>::new();
        let base_addr: usize = unsafe { FAKE_HARDWARE_MEM.0.as_ptr() as usize };

        for exp in 12..TEST_EXPONENT {
            let layout = Layout::from_size_align(TEST_MEM_SIZE, TEST_MEM_SIZE).unwrap();

            allocator.init(base_addr, TEST_MEM_SIZE);
            let ptr = allocator.alloc(layout).unwrap();
            assert_eq!(allocator.available(), 0);

            allocator.free(ptr, layout);
            assert_eq!(allocator.available(), TEST_MEM_SIZE);
        }
    }
}
