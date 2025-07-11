use core::{
    mem::{self, transmute},
    ptr::{copy_nonoverlapping, null_mut},
};

use axhal::{
    mem::{MemoryAddr, VirtAddr},
    paging::MappingFlags,
};
use object::File;
use object::Object;
use object::ObjectSection;
use uefi_raw::{protocol::console::SimpleTextOutputProtocol, table::{system::SystemTable, Header}, Status};

extern crate alloc;
use alloc::boxed::Box;

pub mod efi;
pub mod protocol;
pub mod table;

pub type EfiMainFn =
    extern "efiapi" fn(image_handle: *mut core::ffi::c_void, system_table: *mut SystemTable) -> u64;

extern "efiapi" fn output_string(
    _this: *mut SimpleTextOutputProtocol,
    string: *const u16,
) -> Status {
    unsafe {
        let mut len = 0;
        while *string.add(len) != 0 {
            len += 1;
        }
        let message = core::slice::from_raw_parts(string, len as usize).iter();
        let utf16_message = core::char::decode_utf16(message.cloned());
        let decoded_message: alloc::string::String =
            utf16_message.map(|r| r.unwrap_or('\u{FFFD}')).collect();
        info!("EFI Output: {}", decoded_message);
    }
    uefi_raw::Status(0)
}

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
    
    let stdout = unsafe {
        Box::into_raw(Box::new(SimpleTextOutputProtocol {
            reset: mem::transmute(0usize),
            output_string: output_string,
            test_string: mem::transmute(0usize),
            query_mode: mem::transmute(0usize),
            set_mode: mem::transmute(0usize),
            set_attribute: mem::transmute(0usize),
            clear_screen: mem::transmute(0usize),
            set_cursor_position: mem::transmute(0usize),
            enable_cursor: mem::transmute(0usize),
            mode: core::ptr::null_mut(),
        }))
    };

    let system_table = Box::into_raw(Box::new(SystemTable {
        header: Header::default(),

        firmware_vendor: null_mut(),
        firmware_revision: 0,

        stdin_handle: null_mut(),
        stdin: null_mut(),

        stdout_handle: null_mut(),
        stdout,

        stderr_handle: null_mut(),
        stderr: null_mut(),

        runtime_services: null_mut(),
        boot_services: null_mut(),

        number_of_configuration_table_entries: 0,
        configuration_table: null_mut(),
    }));

    let result = func(null_mut(), system_table);
    info!("efi_main return: {}", result)
}
