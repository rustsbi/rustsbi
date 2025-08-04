extern crate alloc;

use object::{File, Object, ObjectSection};

pub fn load_efi_file(path: &str) -> alloc::vec::Vec<u8> {
    axfs::api::read(path).expect("Failed to read EFI file from ramdisk")
}

pub fn parse_efi_file(data: &[u8]) -> File {
    File::parse(data).expect("Failed to parse EFI file")
}

pub fn analyze_sections(file: &File) -> (u64, u64) {
    let mut base_va = u64::MAX;
    let mut max_va = 0;

    for section in file.sections() {
        let size = section.size();
        let start = section.address();
        let end = start + size;
        base_va = base_va.min(start);
        max_va = max_va.max(end);
    }

    (base_va, max_va)
}

pub fn load_sections(file: &File, mapping: *mut u8, base_va: u64) {
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
                core::ptr::copy_nonoverlapping(data.as_ptr(), mapping.add(offset), data.len());
            }
        }
    }
}
