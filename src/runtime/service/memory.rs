use alloc::vec::Vec;
use axhal::{
    mem::{MemoryAddr, PhysAddr, VirtAddr},
    paging::MappingFlags,
};
use axsync::Mutex;
use uefi_raw::table::boot::MemoryType;

static ALLOCATED_PAGES: Mutex<Vec<(VirtAddr, usize)>> = Mutex::new(Vec::new());
static ALLOCATED_POOLS: Mutex<Vec<(usize, core::alloc::Layout)>> = Mutex::new(Vec::new());

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AllocateType {
    AnyPages = 0,   // AllocateAnyPages
    MaxAddress = 1, // AllocateMaxAddress
    Address = 2,    // AllocateAddress
}

impl TryFrom<u32> for AllocateType {
    type Error = ();

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(AllocateType::AnyPages),
            1 => Ok(AllocateType::MaxAddress),
            2 => Ok(AllocateType::Address),
            _ => Err(()),
        }
    }
}

impl From<AllocateType> for u32 {
    fn from(v: AllocateType) -> u32 {
        v as u32
    }
}

pub fn alloc_pages(_alloc_type: AllocateType, _memory_type: MemoryType, count: usize) -> *mut u8 {
    let layout = core::alloc::Layout::from_size_align(count * 4096, 4096)
        .expect("Invalid layout for allocate_pages");
    let ptr = axalloc::global_allocator()
        .alloc(layout)
        .expect("Failed to allocate pages for EFI")
        .as_ptr();

    let page_count = (layout.size() + 4095) / 4096;

    axmm::kernel_aspace()
        .lock()
        .protect(
            VirtAddr::from_ptr_of(ptr).align_down(4096usize),
            page_count * 4096,
            MappingFlags::READ | MappingFlags::WRITE | MappingFlags::EXECUTE,
        )
        .expect("Failed to protect EFI memory");

    ptr
}

pub fn free_pages(_addr: PhysAddr, _page: usize) {}

pub fn allocate_pool(_memory_type: MemoryType, size: usize) -> *mut u8 {
    if size == 0 {
        return core::ptr::null_mut();
    }
    // UEFI requires at least 8-byte alignment for pool allocations.
    let layout = match core::alloc::Layout::from_size_align(size, 8) {
        Ok(l) => l,
        Err(_) => return core::ptr::null_mut(),
    };
    let ptr = match axalloc::global_allocator().alloc(layout) {
        Ok(nn) => nn.as_ptr(),
        Err(_) => return core::ptr::null_mut(),
    };
    ALLOCATED_POOLS.lock().push((ptr as usize, layout));
    ptr
}

pub fn free_pool(buffer: *mut u8) {
    if buffer.is_null() {
        return;
    }
    let addr = buffer as usize;
    let mut pools = ALLOCATED_POOLS.lock();
    if let Some(idx) = pools.iter().position(|(p, _)| *p == addr) {
        let (_, layout) = pools.swap_remove(idx);
        // Safety: pointer/layout came from our allocator.
        unsafe {
            axalloc::global_allocator().dealloc(
                core::ptr::NonNull::new_unchecked(buffer),
                layout,
            )
        };
    }
}
