use axhal::{
    mem::{MemoryAddr, VirtAddr},
    paging::MappingFlags,
};

pub fn alloc_and_map_memory(size: usize, source_data: &[u8]) -> *mut u8 {
    let layout = core::alloc::Layout::from_size_align(source_data.len(), 4096)
        .expect("Invalid layout for shellcode");

    let ptr = axalloc::global_allocator()
        .alloc(layout)
        .expect("Failed to allocate memory for shellcode")
        .as_ptr();

    let page_count = (size + 4095) / 4096;

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
