//! Secure Memory Management Core Traits
//!
//! Abstractions for TEE memory management and hardware isolation.
//! Defines **SecMemManager** (orchestration), **SecMemAllocator** (allocation logic),
//! and **SecMemProtector** (hardware HAL) for **RT** and **APP** enclaves.
//! Serves as the base for Penglai/Keystone-style secure memory implementations.
#![no_std]
extern crate alloc;
use alloc::vec::Vec;
use core::ptr::NonNull;
use core::{alloc::Layout, usize};
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
    /// Sanitized region awaiting confirmation that its protection was removed.
    None,
    /// Region available for either kind of allocator.
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

struct SecMemRegion<const ORDER: usize, AR, AA>
where
    AR: SecMemAllocator<ORDER>,
    AA: SecMemAllocator<ORDER>,
{
    addr: usize,
    len: usize,
    id: usize,
    slot: u32,
    live_allocations: usize,
    // Two harts can grant the same allocation; each revocation removes one occurrence.
    granted_allocations: Vec<usize>,
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
    /// Returns whether `addr` and `len` describe one complete live allocation.
    fn owns(&self, _addr: usize, _len: usize) -> bool {
        false
    }
    /// Releases a live allocation with its original layout.
    /// The manager checks [`Self::owns`] and zeroes the block before calling this method.
    /// The allocator must still be able to release it after its contents are erased.
    fn free(&mut self, ptr: NonNull<u8>, layout: Layout) -> bool {
        let _ = (ptr, layout);
        false
    }
    /// Returns an upper bound on bytes available for another allocation.
    /// A particular layout may still fail because of alignment or fragmentation.
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
            live_allocations: 0,
            granted_allocations: Vec::new(),
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
    /// Takes ownership of the first 16 PMP entries on every affected hart.
    ///
    /// # Safety
    ///
    /// The entries must be implemented, unlocked, and initially OFF. Use
    /// standard PMP permissions.
    /// Serialize all updates with affected contexts and other accesses to the
    /// packed CSRs. Both allocators must accurately track nonoverlapping live
    /// allocations and support freeing a validated block after sanitization.
    /// `update_local` must update this hart and `update_all` every hart that
    /// can access the registered regions. Both must honor [`protector::UpdatePmp`].
    /// A single-hart platform can pass the same callback twice.
    unsafe fn new(update_local: protector::UpdatePmp, update_all: protector::UpdatePmp) -> Self;
    /// Protects the monitor before enabling the default access rule.
    ///
    /// # Safety
    ///
    /// The range must contain the monitor's code, data and all other memory
    /// that must stay inaccessible to lower privilege modes. Outside this
    /// range, lower modes gain RWX access unless a higher-priority rule applies.
    /// Keep lower-mode contexts quiescent until initialization succeeds.
    unsafe fn init(&mut self, sm_addr: usize, sm_len: usize) -> bool;
    /// Erases registered regions, including their live allocations, and writes the
    /// released ranges to the first `count` elements of `cur_regions`, returning `count`.
    /// Stops at the first region with an outstanding grant or a failed PMP update.
    /// A region already erased stays reserved until its protection can be removed.
    /// The monitor and default PMP entries remain active.
    fn deinit(&mut self, cur_regions: &mut [(usize, usize); MAX_PMP_ENTRY_COUNT as usize]) -> u32;
    /// Registers a writable region and denies lower-mode access on all affected harts.
    ///
    /// # Safety
    ///
    /// The caller must ensure that, until the region is handed back by
    /// [`Self::reclaim`] or [`Self::deinit`]:
    ///
    /// - `addr + len` does not overflow, and `addr..addr + len` is valid for
    ///   writes.
    /// - Give the manager exclusive use of the range except for the allocations
    ///   it grants. Exclude conflicting access, including DMA. End outstanding
    ///   borrows before an allocation or the whole region is sanitized.
    ///
    /// # Examples
    ///
    /// Registering memory without `unsafe` is rejected at compile time:
    ///
    /// ```compile_fail,E0133
    /// use smm::allocators::NoneAlloc;
    /// use smm::manager::UniSecMemManager;
    /// use smm::SecMemManager;
    ///
    /// fn register(manager: &mut UniSecMemManager<30, NoneAlloc<30>, NoneAlloc<30>>) {
    ///     manager.extend(0x8000_0000, 0x1000);
    /// }
    /// ```
    unsafe fn extend(&mut self, addr: usize, len: usize) -> bool;
    /// Sanitizes and returns a region with no live allocations.
    fn reclaim(&mut self) -> Option<(usize, usize)>;
    /// Allocates at least `len` bytes and returns the address, actual size, and region ID.
    fn alloc_em(&mut self, len: usize, em_type: SecMemType) -> Option<(usize, usize, usize)>;
    /// Clears a live allocation and returns its region ID once every hart has revoked access.
    fn free_em(&mut self, addr: usize, len: usize) -> Option<usize>;
    /// Grants one live allocation on this hart. Revoke it before granting
    /// another, changing the hart's enclave context, or releasing its region.
    fn grant_access(&mut self, addr: usize, len: usize, region_id: usize) -> bool;
    /// Revokes this hart's matching grant. Other harts retain their grants.
    fn retrive_access(&mut self, addr: usize, len: usize, region_id: usize) -> bool;
}

pub trait SecMemProtector {
    /// Alloc a hardware for current region
    fn alloc(&mut self) -> Option<u32>;
    /// Free a hardware
    fn free(&mut self, hwid: u32) -> bool;
    /// Check memory area can be protect by hardware or not.
    ///
    /// This is a hardware geometry check such as PMP NAPOT alignment; a
    /// `true` result does not mean the range is valid Rust memory.
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
}
