use core::{
    mem::transmute,
    ptr::{copy_nonoverlapping, null_mut},
};

use axhal::{
    mem::{MemoryAddr, VirtAddr},
    paging::MappingFlags,
};
use object::File;
use object::Object;
use object::ObjectSection;
use uefi_raw::table::system::SystemTable;

use crate::runtime::table::{get_system_table_raw, init_system_table};

mod protocol;
mod service;
mod table;

pub type EfiMainFn =
    extern "efiapi" fn(image_handle: *mut core::ffi::c_void, system_table: *mut SystemTable) -> u64;

pub fn efi_runtime_init() {
    let shellcode =
        axfs::api::read("/EFI/BOOT/BOOTRISCV64.EFI").expect("Failed to read EFI file from ramdisk");

    let file = File::parse(&*shellcode).unwrap();
    let entry = file.entry();
    info!("Entry point: 0x{:x}", entry);

    let mut base_va = u64::MAX;
    let mut max_va = 0;
    for section in file.sections() {
        let size = section.size();
        let start = section.address();
        let end = start + size;
        base_va = base_va.min(start);
        max_va = max_va.max(end);
    }

    let mem_size = (max_va - base_va) as usize;
    info!(
        "Mapping memory: 0x{:x} bytes at RVA base 0x{:x}",
        mem_size, base_va
    );

    let page_count = (mem_size + 4095) / 4096;

    // alloc memory for the EFI image
    let flags = MappingFlags::READ | MappingFlags::WRITE | MappingFlags::EXECUTE;
    let layout: core::alloc::Layout = core::alloc::Layout::from_size_align(shellcode.len(), 4096)
        .expect("Invalid layout for shellcode");

    let mapping = axalloc::global_allocator()
        .alloc(layout)
        .expect("Failed to allocate memory for shellcode");
    let mapping = mapping.as_ptr();

    axmm::kernel_aspace()
        .lock()
        .protect(
            VirtAddr::from_ptr_of(mapping).align_down(4096usize),
            page_count * 4096,
            flags,
        )
        .expect("Failed to protect efi memory");

    for section in file.sections() {
        if let Ok(data) = section.data() {
            let offset = (section.address() - base_va) as usize;
            info!(
                "Loading section {} to offset 0x{:x}, size 0x{:x}",
                section.name().unwrap_or("<unnamed>"),
                offset,
                data.len()
            );
            unsafe {
                copy_nonoverlapping(data.as_ptr(), (mapping as *mut u8).add(offset), data.len());
            }
        }
    }

    let func_addr = (mapping as usize + (entry - base_va) as usize) as *const ();
    let func: EfiMainFn = unsafe { transmute(func_addr) };

    let system_table = {
        init_system_table();
        get_system_table_raw()
    };

    let result = func(null_mut(), system_table);
    info!("efi_main return: {}", result)
}
