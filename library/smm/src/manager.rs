//! Unified Secure Memory Manager
//!
//! A concrete **SecMemManager** instance for unified TEE memory management.
//! Orchestrates **RT** (exclusive) and **APP** (shared) enclave memory styles.
//! Implements hardware isolation via **protectors** and secure sanitization.
//! Manages **Region** lifecycle from **General** to specialized enclave states.

use super::*;
use crate::protector::TestSecMemProtector;
use alloc::vec::Vec;
use core::usize;

type UniSecMemProtector = TestSecMemProtector;

/// These PMP slots are reserved and not used to alloc enclave mem.
///
/// PMP N-1 : Default grant kernel access with all mem.
/// PMP 1   : Only used in Penglai, for temporarily grant kernel access with specific sec mem.
/// PMP 0   : Protect SM code/data.
const UNI_SECMEM_SM_SLOT: u32 = 1 << 0;
const UNI_SECMEM_TEMP_SLOT: u32 = 1 << 1;
const UNI_SECMEM_DEFAULT_SLOT: u32 = 1 << (MAX_PMP_ENTRY_COUNT - 1);
const UNI_SECMEM_PMP_ALLOCMASK: u64 = ((1 << MAX_PMP_ENTRY_COUNT) - 1)
    & !((UNI_SECMEM_SM_SLOT | UNI_SECMEM_TEMP_SLOT | UNI_SECMEM_TEMP_SLOT) as u64);
const UNI_SECMEM_PMP_MANAGEMASK: u64 = UNI_SECMEM_PMP_ALLOCMASK
    & ((UNI_SECMEM_SM_SLOT | UNI_SECMEM_DEFAULT_SLOT | UNI_SECMEM_TEMP_SLOT) as u64);

pub struct UniSecMemManager<const ORDER: usize, AR, AA>
where
    AR: SecMemAllocator<ORDER>,
    AA: SecMemAllocator<ORDER>,
{
    /// A simple incrementing ID for region
    cur_idx: usize,
    ///
    protector: UniSecMemProtector,

    /// These PMP slots are reserved and not used to alloc enclave mem.
    reserved_regions: Vec<SecMemRegion<ORDER, NoneAlloc<ORDER>, NoneAlloc<ORDER>>>,
    /// These region are used to manage allocable secure mem.
    ///
    /// Use PMP 2~(N-2)
    alloc_regions: Vec<SecMemRegion<ORDER, AR, AA>>,
}

impl<const ORDER: usize, AR, AA> SecMemManager<ORDER, AR, AA> for UniSecMemManager<ORDER, AR, AA>
where
    AR: SecMemAllocator<ORDER>,
    AA: SecMemAllocator<ORDER>,
{
    fn new() -> Self {
        Self {
            cur_idx: 0,
            protector: UniSecMemProtector::new(UNI_SECMEM_PMP_ALLOCMASK, UNI_SECMEM_PMP_MANAGEMASK),
            reserved_regions:
                Vec::<SecMemRegion<ORDER, NoneAlloc<ORDER>, NoneAlloc<ORDER>>>::with_capacity(
                    1usize,
                ),
            alloc_regions: Vec::<SecMemRegion<ORDER, AR, AA>>::with_capacity(
                (MAX_PMP_ENTRY_COUNT - 3) as usize,
            ),
        }
    }
    fn init(&mut self, sm_addr: usize, sm_len: usize) -> bool {
        // Reset temporary and default PMP slot
        if !self.protector.disable(UNI_SECMEM_TEMP_SLOT)
            || !self
                .protector
                .grant_access_all(0, usize::MAX, UNI_SECMEM_DEFAULT_SLOT)
        {
            error!("[SMM] Reset Temporary/Default PMP slot failed");
            return false;
        }

        // Init and protect SM region
        if self.protector.is_protectable(sm_addr, sm_len)
            && self
                .protector
                .retrive_access_all(sm_addr, sm_len, UNI_SECMEM_SM_SLOT)
        {
            self.reserved_regions.push(
                SecMemRegion::<ORDER, NoneAlloc<ORDER>, NoneAlloc<ORDER>>::new(
                    sm_addr,
                    sm_len,
                    self.cur_idx,
                    UNI_SECMEM_SM_SLOT,
                ),
            );
        } else {
            error!("[SMM] Cannot protect SM region due to memory check or protect fail");
            return false;
        }
        self.cur_idx += 1;
        true
    }

    fn deinit(mut self, cur_regions: &mut [(usize, usize); MAX_PMP_ENTRY_COUNT as usize]) -> u32 {
        let mut count = 0;
        for region in self.alloc_regions.drain(..) {
            // Though SMM assume all enclaves should exited successfully before deinit, which means
            // there isn't any protected data in secure memory. But TEE driver may also be attacked,
            // so SMM still clean all data in secure memory when deinit to avoid any protected data leap.
            unsafe {
                core::ptr::write_bytes(region.addr as *mut u8, 0, region.len);
                // sfence_vma_all();
            }
            // Disable hardware used to protect region.
            cur_regions[count] = (region.addr, region.len);
            count += 1;
            if !self.protector.disable(region.slot) {
                error!(
                    "[SMM] Deinit Region-{} with slot-{} fail",
                    region.id, region.slot
                );
            }
        }
        count as u32
    }

    fn extend(&mut self, addr: usize, len: usize) -> bool {
        // Check if new memory area can be protect by hardware
        if !self.protector.is_protectable(addr, len)
            || (self.alloc_regions.len() >= self.alloc_regions.capacity())
            // New region's mem area shouldn't overlap with any exist region
            || self
                .alloc_regions
                .iter()
                .any(|r| r.is_mem_overlap(addr, len))
        {
            return false;
        }

        // Try alloc a hardware to protect new region
        if let Some(hwid) = self.protector.alloc() {
            if !self.protector.retrive_access_all(addr, len, hwid) {
                self.protector.free(hwid);
                return false;
            }
            self.cur_idx += 1;
            self.alloc_regions.push(SecMemRegion::<ORDER, AR, AA>::new(
                addr,
                len,
                self.cur_idx,
                hwid,
            ));
        }
        true
    }

    fn reclaim(&mut self) -> Option<(usize, usize)> {
        let index = self
            .alloc_regions
            .iter()
            .position(|region| region.is_used == false)?;
        let region = self.alloc_regions.swap_remove(index);
        // Sanitization is done when free enclave memory. And in-use region won't be reclaim, so
        // no need clean secure memory.
        // unprotect region and try free PMP slot
        if self.protector.disable(region.slot) && self.protector.free(region.slot) {
            return Some((region.addr, region.len));
        }
        None
    }

    fn alloc_em(&mut self, len: usize, em_type: SecMemType) -> Option<(usize, usize, usize)> {
        // Init alloc layout, size must be power of 2
        let mut expect_len = len.next_power_of_two();

        for region in self.alloc_regions.iter_mut() {
            if region.len < len {
                continue;
            }

            // If region is General, then change it to request type
            // BEWARE that alloc should success in most of time, so change of type won't reverse even when alloc failed,
            // if region reach limits, it depends on TEE driver to reclaim unused regions manully.
            if matches!(region.allocator, SecMemAllocatorWrapper::General) {
                if matches!(em_type, SecMemType::Runtime) {
                    let mut alloc = AR::new();
                    alloc.init(region.addr, region.len);
                    region.allocator = SecMemAllocatorWrapper::Runtime(alloc);
                } else if matches!(em_type, SecMemType::Application) {
                    let mut alloc = AA::new();
                    alloc.init(region.addr, region.len);
                    region.allocator = SecMemAllocatorWrapper::Application(alloc);
                }
            }

            // If region type is equal to request type, try alloc enclave mem
            if let Ok(ptr) = match &mut region.allocator {
                SecMemAllocatorWrapper::Runtime(alloc)
                    if matches!(em_type, SecMemType::Runtime)
                    // Pre-allocation capacity check
                        && (alloc.available() >= expect_len) =>
                {
                    expect_len = alloc.available();
                    alloc.alloc(Layout::from_size_align(expect_len, expect_len).ok()?)
                }
                SecMemAllocatorWrapper::Application(alloc)
                    if matches!(em_type, SecMemType::Application)
                    // Pre-allocation capacity check
                        && (alloc.available() >= expect_len) =>
                {
                    alloc.alloc(Layout::from_size_align(expect_len, expect_len).ok()?)
                }
                _ => Err(()),
            } {
                // If alloc successfully, get addr and idx of region
                region.is_used = true;
                return Some((ptr.as_ptr() as usize, expect_len, region.id));
            }
        }
        None
    }

    fn free_em(&mut self, addr: usize, len: usize) -> Option<usize> {
        let expect_len = len.next_power_of_two();
        let free_layout = Layout::from_size_align(expect_len, expect_len).ok()?;
        let free_ptr = NonNull::new(addr as *mut u8)?;

        if let Some(free_region) = self
            .alloc_regions
            .iter_mut()
            .find(|region| region.is_mem_contained(addr, len))
        {
            // Clean enclave memory to avoid data leap between enclaves and when host request reclaim
            unsafe {
                core::ptr::write_bytes(addr as *mut u8, 0, len);
                // sfence_vma_all();
            }
            return match &mut free_region.allocator {
                SecMemAllocatorWrapper::Application(alloc) => {
                    alloc.free(free_ptr, free_layout);
                    // if all memory of region is free, reset region status.
                    if alloc.available() == alloc.total() {
                        free_region.is_used = false;
                        free_region.allocator = SecMemAllocatorWrapper::General;
                    }
                    Some(free_region.id)
                }
                SecMemAllocatorWrapper::Runtime(alloc) => {
                    alloc.free(free_ptr, free_layout);
                    // if all memory of region is free, reset region status.
                    if alloc.available() == alloc.total() {
                        free_region.is_used = false;
                        free_region.allocator = SecMemAllocatorWrapper::General;
                    }
                    Some(free_region.id)
                }
                _ => None,
            };
        }
        None
    }

    /// Grant enclave access to certain region on current hart
    fn grant_access(&self, addr: usize, len: usize, region_id: usize) -> bool {
        // find region match enclave memory and region id, grant access to region on current hart
        if let Some(enclave_region) = self
            .alloc_regions
            .iter()
            .find(|region| region.id == region_id && region.is_mem_contained(addr, len))
        {
            return self.protector.grant_access(
                enclave_region.addr,
                enclave_region.len,
                enclave_region.slot,
            );
        }
        false
    }
    /// Retrive enclave access to certain region on current hart
    fn retrive_access(&self, addr: usize, len: usize, region_id: usize) -> bool {
        if let Some(enclave_region) = self
            .alloc_regions
            .iter()
            .find(|region| region.id == region_id && region.is_mem_contained(addr, len))
        {
            return self.protector.retrive_access(
                enclave_region.addr,
                enclave_region.len,
                enclave_region.slot,
            );
        }
        false
    }
}

#[cfg(test)]
mod smm_stress_tests {
    extern crate std;
    use crate::allocators::AppAlloc;
    use crate::allocators::RTAlloc;

    use super::*;
    use rand::{Rng, SeedableRng, rngs::StdRng};
    use std::println;
    use std::vec;

    // 常量定义
    const POOL_SIZE: usize = 512 * 1024 * 1024; // 512MB
    const MAX_ALLOC: usize = 16 * 1024 * 1024; // 16MB
    const REGION_COUNT: usize = 8; // 8个Region，每个64MB

    struct StdOut;
    impl Write for StdOut {
        fn write_str(&mut self, s: &str) -> Result {
            std::print!("{}", s);
            Ok(())
        }
    }
    use core::fmt::{Result, Write};
    impl<const ORDER: usize, AR, AA> UniSecMemManager<ORDER, AR, AA>
    where
        AR: SecMemAllocator<ORDER>,
        AA: SecMemAllocator<ORDER>,
    {
        /// Print all region's information in current manager.
        pub fn dump_to<W: Write>(&self, w: &mut W) -> Result {
            writeln!(w, "\n--- [UniSecMemManager Dump] ---")?;
            writeln!(
                w,
                "Total Configured Regions: {}",
                self.alloc_regions.len() + self.reserved_regions.len()
            )?;

            // 1. 打印 SM 预留区域
            writeln!(w, "\n[Reserved Regions (SM)]")?;
            for r in &self.reserved_regions {
                writeln!(
                    w,
                    "  ID: {:2} | Range: [0x{:016x} - 0x{:016x}] | PMP_Slot: {:<2} | Type: SM/Reserved",
                    r.id,
                    r.addr,
                    r.addr + r.len,
                    r.slot
                )?;
            }

            // 2. 打印可分配区域
            writeln!(w, "\n[Allocatable Regions]")?;
            if self.alloc_regions.is_empty() {
                writeln!(w, "  (None)")?;
            }

            for r in &self.alloc_regions {
                let (type_str, used, total) = match &r.allocator {
                    SecMemAllocatorWrapper::General => ("General    ", 0, r.len),
                    SecMemAllocatorWrapper::Runtime(alloc) => (
                        "Runtime    ",
                        alloc.total() - alloc.available(),
                        alloc.total(),
                    ),
                    SecMemAllocatorWrapper::Application(alloc) => (
                        "Application",
                        alloc.total() - alloc.available(),
                        alloc.total(),
                    ),
                    SecMemAllocatorWrapper::None => ("None", 0, 0),
                };

                let usage_pcnt = if total > 0 { (used * 100) / total } else { 0 };
                let status = if r.is_used { "IN_USE" } else { "IDLE  " };

                writeln!(
                    w,
                    "  ID: {:2} | Range: [0x{:016x} - 0x{:016x}] | PMP: {:<2} | [{}] | Type: {} | Usage: {:3}% ({:0x}  / {:0x})",
                    r.id,
                    r.addr,
                    r.addr + r.len,
                    r.slot,
                    status,
                    type_str,
                    usage_pcnt,
                    used,
                    total
                )?;
            }
            writeln!(w, "--- [End of Dump] ---\n")
        }
    }
    #[test]
    fn test_uni_secmem_manager_stress_aligned() {
        let mut raw_buffer = vec![0u8; POOL_SIZE * 2];
        let raw_addr = raw_buffer.as_mut_ptr() as usize;
        let mut out = StdOut {};

        let align_mask = POOL_SIZE - 1;
        let aligned_base = (raw_addr + align_mask) & !align_mask;

        assert_eq!(
            aligned_base % POOL_SIZE,
            0,
            "Base address is not aligned to size"
        );
        assert!(
            aligned_base + POOL_SIZE <= raw_addr + (POOL_SIZE * 2),
            "Remaining space insufficient"
        );

        println!("Memory Pool Info:");
        println!("  Raw Buffer:  0x{:x}", raw_addr);
        println!("  Aligned Base: 0x{:x}", aligned_base);
        println!("  Pool End:    0x{:x}", aligned_base + POOL_SIZE);

        let mut manager = UniSecMemManager::<30, RTAlloc<30>, AppAlloc<30>>::new();

        assert!(manager.init(0x1000, 0x1000));

        let region_len = POOL_SIZE / REGION_COUNT;
        for i in 0..REGION_COUNT {
            let addr = aligned_base + (i * region_len);
            assert!(
                manager.extend(addr, region_len),
                "Extend failed at region {}",
                i
            );
        }

        let mut rng = StdRng::seed_from_u64(42);
        let mut allocations = Vec::new();
        let iterations = 10000;
        let mut alloc_count = 0;

        for i in 0..iterations {
            if rng.gen_bool(0.7) || allocations.is_empty() {
                let em_type = if rng.gen_bool(0.5) {
                    SecMemType::Application
                } else {
                    SecMemType::Application
                };

                let exponent = rng.gen_range(12..MAX_ALLOC.ilog2());
                let size = 1usize << exponent;

                if let Some((addr, actual_len, id)) = manager.alloc_em(size, em_type) {
                    unsafe {
                        let ptr = addr as *mut u8;
                        core::ptr::write_bytes(ptr, 0x1F, actual_len);
                        assert_eq!(core::ptr::read_volatile(ptr), 0x1F);
                    }
                    alloc_count += 1;
                    allocations.push((addr, actual_len, em_type));
                }
            } else {
                let idx = rng.gen_range(0..allocations.len());
                let (addr, len, _) = allocations.remove(idx);
                assert!(manager.free_em(addr, len).is_some());

                unsafe {
                    assert_eq!(
                        *(addr as *const u8),
                        0,
                        "Memory sanitization failed at 0x{:x}",
                        addr
                    );
                }
            }

            if i % 500 == 0 {
                println!("  Iter {}: Allocated blocks = {}", i, allocations.len());
                let _ = manager.dump_to(&mut out);
            }
        }

        println!("Finalizing: Cleaning up all blocks...");
        for (addr, len, _) in allocations {
            manager.free_em(addr, len);
        }
        let _ = manager.dump_to(&mut out);

        let mut reclaimed_count = 0;
        while let Some(_) = manager.reclaim() {
            reclaimed_count += 1;
        }

        assert_eq!(
            reclaimed_count, REGION_COUNT,
            "State machine error: Not all regions returned to General"
        );

        let mut cur_regions = [(0usize, 0usize); MAX_PMP_ENTRY_COUNT as usize];
        let deinit_count = manager.deinit(&mut cur_regions);
        println!(
            "Test Passed: All {} regions reclaimed, deinit count {}, success alloc {}, total request {}.",
            reclaimed_count, deinit_count, alloc_count, iterations
        );
    }
}
