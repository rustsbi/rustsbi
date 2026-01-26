use object::Object;
use uefi_raw::table::boot::MemoryType;

use crate::runtime::service::memory::AllocateType;

mod entry;
mod loader;
mod protocol;
mod service;
mod system_table;
mod utils;

pub fn efi_runtime_init() {
    // Prepare the UEFI System Table
    let system_table = {
        system_table::init_system_table();
        system_table::get_system_table_raw()
    };

    // Load the bootloader EFI file
    let load_bootloader = loader::load_efi_file("/EFI/BOOT/BOOTRISCV64.EFI");
    let meta = loader::detect_pe_meta(&load_bootloader).expect("Failed to parse PE metadata");

    let file = loader::parse_efi_file(&load_bootloader);
    let mem_size = meta.size_of_image as usize;

    // let mapping = crate::runtime::service::memory::alloc_and_map_memory(mem_size, &load_bootloader);
    let mapping = crate::runtime::service::memory::alloc_pages(
        AllocateType::AnyPages,
        MemoryType::LOADER_CODE,
        mem_size / 4096 + 1,
    );

    loader::load_image(&load_bootloader, &file, mapping, meta);
    loader::apply_relocations(
        &file,
        mapping,
        mapping as u64,
        meta.image_base,
        meta.size_of_image,
    );

    // Sanity-check the entry semantics from `object` (it should be image_base + entry_rva).
    let obj_entry = file.entry();
    let expected_entry = meta.image_base.wrapping_add(meta.entry_rva as u64);
    if obj_entry != expected_entry {
        warn!(
            "Unexpected PE entry: object=0x{:x}, expected(image_base+entry_rva)=0x{:x}",
            obj_entry, expected_entry
        );
    }

    // RISC-V needs an I-cache sync after writing code into memory.
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    unsafe {
        core::arch::asm!("fence.i");
    }

    info!(
        "Loaded EFI file: machine=0x{:04x}, preferred_image_base=0x{:x}, loaded_image_base=0x{:x}, size_of_image=0x{:x}, entry_rva=0x{:x}, mapping {:?}",
        meta.machine, meta.image_base, mapping as u64, meta.size_of_image, meta.entry_rva, mapping,
    );

    let func = entry::resolve_entry_func(mapping, meta.entry_rva as u64);
    // UEFI applications often expect a non-null ImageHandle (gImageHandle) for library init.
    static mut DUMMY_IMAGE_HANDLE: usize = 1;
    let image_handle = core::ptr::addr_of_mut!(DUMMY_IMAGE_HANDLE) as *mut core::ffi::c_void;
    let result = func(image_handle, system_table);
    info!("efi_main return: {}", result);
}
