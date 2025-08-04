use object::Object;

mod entry;
mod loader;
mod memory;
mod protocol;
mod service;
mod table;

pub fn efi_runtime_init() {
    let load_bootloader = loader::load_efi_file("/EFI/BOOT/BOOTRISCV64.EFI");
    let image_base =
        loader::detect_and_get_image_base(&load_bootloader).expect("Failed to get PE image base");

    let file = loader::parse_efi_file(&load_bootloader);
    let (base_va, max_va) = loader::analyze_sections(&file);
    let mem_size = (max_va - base_va) as usize;

    let mapping = memory::alloc_and_map_memory(mem_size, &load_bootloader);

    loader::load_sections(&file, mapping, base_va);
    loader::apply_relocations(&file, mapping, base_va, image_base);

    info!("Loaded EFI file with base VA: 0x{:x}, max VA: 0x{:x}, image base {:x}", base_va, max_va, image_base);

    let func = entry::resolve_entry_func(mapping, file.entry(), base_va);

    let system_table = {
        table::init_system_table();
        table::get_system_table_raw()
    };

    let result = func(core::ptr::null_mut(), system_table);
    info!("efi_main return: {}", result);
}
