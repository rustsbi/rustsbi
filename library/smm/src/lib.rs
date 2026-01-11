//! Secure Memory Management Core Traits
//!
//! Abstractions for TEE memory management and hardware isolation.
//! Defines **SecMemManager** (orchestration), **SecMemAllocator** (allocation logic),
//! and **SecMemProtector** (hardware HAL) for **RT** and **APP** enclaves.
//! Serves as the base for Penglai/Keystone-style secure memory implementations.
#![no_std]
extern crate alloc;
use crate::allocators::NoneAlloc;
use core::ptr::NonNull;
use core::{alloc::Layout, usize};
use log::error;
use pmpm::MAX_PMP_ENTRY_COUNT;

pub mod allocators;
pub mod manager;
pub mod protector;

#[derive(Clone, Copy, PartialEq)]
pub enum SecMemAllocatorWrapper<const ORDER: usize, AR, AA>
where
    AR: SecMemAllocator<ORDER>,
    AA: SecMemAllocator<ORDER>,
{
    None,
    General,
    Runtime(AR),
    Application(AA),
}

#[derive(Clone, Copy, PartialEq)]
pub enum SecMemType {
    None,
    General,
    Runtime,
    Application,
}

#[derive(Clone, Copy)]
struct SecMemRegion<const ORDER: usize, AR, AA>
where
    AR: SecMemAllocator<ORDER>,
    AA: SecMemAllocator<ORDER>,
{
    addr: usize,
    len: usize,
    id: usize,
    slot: u32,
    is_used: bool,
    pub allocator: SecMemAllocatorWrapper<ORDER, AR, AA>,
}

pub trait SecMemAllocator<const ORDER: usize> {
    /// Create new allocator
    fn new() -> Self;
    /// Init allocator with specific memory region.
    fn init(&mut self, addr: usize, len: usize) {
        let _ = (addr, len);
    }
    /// Alloc mem from allocator.
    fn alloc(&mut self, layout: Layout) -> Result<NonNull<u8>, ()> {
        let _ = layout;
        Err(())
    }
    /// Free mem to allocator.
    fn free(&mut self, ptr: NonNull<u8>, layout: Layout) {
        let _ = (ptr, layout);
    }
    /// Avaliable mem can be alloced.
    fn available(&self) -> usize {
        0
    }
    /// Total mem.
    fn total(&self) -> usize {
        0
    }
}

impl<const ORDER: usize, AR, AA> SecMemRegion<ORDER, AR, AA>
where
    AR: SecMemAllocator<ORDER>,
    AA: SecMemAllocator<ORDER>,
{
    /// Create new region
    pub fn new(addr: usize, len: usize, id: usize, pmp_slot: u32) -> Self {
        Self {
            addr,
            len,
            id,
            slot: pmp_slot,
            is_used: false,
            allocator: SecMemAllocatorWrapper::General,
        }
    }

    /// If target region is overlap with current mem region.
    #[inline]
    pub fn is_mem_overlap(&self, addr: usize, len: usize) -> bool {
        let self_end = self.addr + self.len;
        if let Some(target_end) = addr.checked_add(len) {
            return addr < self_end && self.addr < target_end;
        }
        true
    }

    /// If current mem region contains target region.
    #[inline]
    pub fn is_mem_contained(&self, addr: usize, len: usize) -> bool {
        let self_end = self.addr + self.len;
        if let Some(target_end) = addr.checked_add(len) {
            return addr >= self.addr && target_end <= self_end;
        }
        false
    }
}

pub trait SecMemManager<const ORDER: usize, AR, AA>
where
    AR: SecMemAllocator<ORDER>,
    AA: SecMemAllocator<ORDER>,
{
    /// Create new manager with no metadata
    fn new() -> Self;
    /// Init new manager
    fn init(&mut self, sm_addr: usize, sm_len: usize) -> bool;
    /// Delete manager, free all resource.
    fn deinit(self, cur_regions: &mut [(usize, usize); MAX_PMP_ENTRY_COUNT as usize]) -> u32;
    /// Create new region with mem.
    fn extend(&mut self, addr: usize, len: usize) -> bool;
    /// Reclaim an unused allocable region from manager
    fn reclaim(&mut self) -> Option<(usize, usize)>;
    /// Alloc enclave mem from request region of type
    fn alloc_em(&mut self, len: usize, em_type: SecMemType) -> Option<(usize, usize, usize)>;
    /// Free enclave mem back to origin region
    fn free_em(&mut self, addr: usize, len: usize) -> Option<usize>;
    /// Grant access to certain memory area on current hart.
    fn grant_access(&self, addr: usize, len: usize, region_id: usize) -> bool;
    /// Retrive access to certain memory area on current hart.
    fn retrive_access(&self, addr: usize, len: usize, region_id: usize) -> bool;
}

pub trait SecMemProtector {
    /// Alloc a hardware for current region
    fn alloc(&mut self) -> Option<u32>;
    /// Free a hardware
    fn free(&mut self, hwid: u32) -> bool;
    /// Check memory area can be protect by hardware or not.
    fn is_protectable(&self, addr: usize, len: usize) -> bool;
    /// Grant access to secure mem on current hart
    fn grant_access(&self, addr: usize, len: usize, hwid: u32) -> bool;
    /// Retrive access to secure mem on current hart
    fn retrive_access(&self, addr: usize, len: usize, hwid: u32) -> bool;
    /// Grant access to secure mem on all hart
    fn grant_access_all(&self, addr: usize, len: usize, hwid: u32) -> bool;
    /// Retrive access to secure mem on all hart
    fn retrive_access_all(&self, addr: usize, len: usize, hwid: u32) -> bool;
    /// Disable hardware
    fn disable(&self, hwid: u32) -> bool;
    // Enable hardware
    fn enable(&self, hwid: u32) -> bool;
}
