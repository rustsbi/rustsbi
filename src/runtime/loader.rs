use object::FileKind;
use object::read::pe::ImageOptionalHeader;
use object::{File, Object, ObjectSection, pe, read::pe::ImageNtHeaders};

pub fn load_efi_file(path: &str) -> alloc::vec::Vec<u8> {
    axfs::api::read(path).expect("Failed to read EFI file from ramdisk")
}

pub fn parse_efi_file(data: &[u8]) -> File {
    File::parse(data).expect("Failed to parse EFI file")
}

pub fn get_pe_image_base<Pe: ImageNtHeaders>(data: &[u8]) -> Option<u64> {
    let mut offset = pe::ImageDosHeader::parse(data)
        .ok()?
        .nt_headers_offset()
        .into();
    let (nt_headers, _) = Pe::parse(data, &mut offset).ok()?;
    Some(nt_headers.optional_header().image_base())
}

pub fn detect_and_get_image_base(data: &[u8]) -> Option<u64> {
    let kind = FileKind::parse(data).ok()?;
    match kind {
        FileKind::Pe32 => get_pe_image_base::<pe::ImageNtHeaders32>(data),
        FileKind::Pe64 => get_pe_image_base::<pe::ImageNtHeaders64>(data),
        _ => None,
    }
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

pub fn apply_relocations(file: &File, mapping: *mut u8, loaded_base: u64, original_base: u64) {
    let delta = loaded_base as i64 - original_base as i64;

    for section in file.sections() {
        if section.name().unwrap_or("") == ".reloc" {
            let data = section.data().unwrap();
            let mut offset = 0;

            while offset < data.len() {
                if offset + 8 > data.len() {
                    break;
                }

                let va = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
                let size = u32::from_le_bytes(data[offset + 4..offset + 8].try_into().unwrap());
                offset += 8;

                let count = (size - 8) / 2;
                for _ in 0..count {
                    if offset + 2 > data.len() {
                        break;
                    }

                    let entry = u16::from_le_bytes(data[offset..offset + 2].try_into().unwrap());
                    offset += 2;

                    let rtype = entry >> 12;
                    let roffset = entry & 0x0fff;

                    let patch_va = va as u64 + roffset as u64;
                    let patch_offset = (patch_va - loaded_base) as usize;

                    match rtype {
                        10 => {
                            // IMAGE_REL_BASED_DIR64
                            let patch_ptr = unsafe { mapping.add(patch_offset) as *mut u64 };
                            unsafe {
                                let orig = patch_ptr.read();
                                patch_ptr.write(orig.wrapping_add(delta as u64));
                            }
                        }
                        3 => {
                            // IMAGE_REL_BASED_HIGHLOW (32-bit)
                            let patch_ptr = unsafe { mapping.add(patch_offset) as *mut u32 };
                            unsafe {
                                let orig = patch_ptr.read();
                                patch_ptr.write(orig.wrapping_add(delta as u32));
                            }
                        }
                        0 => { /* IMAGE_REL_BASED_ABSOLUTE, do nothing */ }
                        _ => {
                            warn!("Unsupported relocation type: {}", rtype);
                        }
                    }
                }
            }
        }
    }
}
