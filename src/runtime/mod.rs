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
use uefi_raw::table::{Header, system::SystemTable};

extern crate alloc;
use alloc::boxed::Box;

use crate::runtime::console::simple_text_output::{get_simple_text_output, init_simple_text_output};

mod console;
mod fs;
mod service;

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
    
    let simple_text_output = {
        init_simple_text_output();
        get_simple_text_output().lock().get_protocol()
    };

    let system_table = Box::into_raw(Box::new(SystemTable {
        header: Header::default(),

        firmware_vendor: null_mut(),
        firmware_revision: 0,

        stdin_handle: null_mut(),
        stdin: null_mut(),

        stdout_handle: null_mut(),
        stdout: simple_text_output,

        stderr_handle: null_mut(),
        stderr: simple_text_output,

        runtime_services: null_mut(),
        boot_services: null_mut(),

        number_of_configuration_table_entries: 0,
        configuration_table: null_mut(),
    }));

    let result = func(null_mut(), system_table);
    info!("efi_main return: {}", result)
}
