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
    let load_bootloader = loader::load_efi_file("/EFI/BOOT/BOOTRISCV64.EFI");
    let image_base =
        loader::detect_and_get_image_base(&load_bootloader).expect("Failed to get PE image base");

    let file = loader::parse_efi_file(&load_bootloader);
    let (base_va, max_va) = loader::analyze_sections(&file);
    let mem_size = (max_va - base_va) as usize;

    // let mapping = crate::runtime::service::memory::alloc_and_map_memory(mem_size, &load_bootloader);
    let mapping = crate::runtime::service::memory::alloc_pages(
        AllocateType::AnyPages,
        MemoryType::LOADER_CODE,
        mem_size / 4096 + 1,
    );

    loader::load_sections(&file, mapping, base_va);
    loader::apply_relocations(&file, mapping, base_va, image_base);

    info!(
        "Loaded EFI file with base VA: 0x{:x}, max VA: 0x{:x}, image base {:x}, mapping {:?}, entry: 0x{:x}",
        base_va,
        max_va,
        image_base,
        mapping,
        file.entry()
    );

    let func = entry::resolve_entry_func(mapping, file.entry(), base_va);

    let system_table = {
        system_table::init_system_table();
        system_table::get_system_table_raw()
    };

    let result = func(core::ptr::null_mut(), system_table);
    info!("efi_main return: {}", result);
}
