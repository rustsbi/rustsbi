use object::FileKind;
use object::endian::LittleEndian;
use object::read::pe::ImageOptionalHeader;
use object::{File, Object, ObjectSection, pe, read::pe::ImageNtHeaders};

#[derive(Debug, Clone, Copy)]
pub struct PeMeta {
    pub machine: u16,
    pub image_base: u64,
    pub entry_rva: u32,
    pub size_of_image: u32,
    pub size_of_headers: u32,
}

pub fn load_efi_file(path: &str) -> alloc::vec::Vec<u8> {
    axfs::api::read(path).expect("Failed to read EFI file from ramdisk")
}

pub fn parse_efi_file(data: &[u8]) -> File {
    File::parse(data).expect("Failed to parse EFI file")
}

fn get_pe_meta_impl<Pe: ImageNtHeaders>(data: &[u8]) -> Option<PeMeta> {
    let mut offset = pe::ImageDosHeader::parse(data)
        .ok()?
        .nt_headers_offset()
        .into();
    let (nt_headers, _) = Pe::parse(data, &mut offset).ok()?;
    let machine = nt_headers.file_header().machine.get(LittleEndian);
    let oh = nt_headers.optional_header();
    Some(PeMeta {
        machine,
        image_base: oh.image_base(),
        entry_rva: oh.address_of_entry_point(),
        size_of_image: oh.size_of_image(),
        size_of_headers: oh.size_of_headers(),
    })
}

pub fn detect_pe_meta(data: &[u8]) -> Option<PeMeta> {
    let kind = FileKind::parse(data).ok()?;
    match kind {
        FileKind::Pe32 => get_pe_meta_impl::<pe::ImageNtHeaders32>(data),
        FileKind::Pe64 => get_pe_meta_impl::<pe::ImageNtHeaders64>(data),
        _ => None,
    }
}

pub fn load_image(data: &[u8], file: &File, mapping: *mut u8, meta: PeMeta) {
    // Copy PE headers (includes DOS header, NT headers, section headers, etc.)
    let headers_len = meta.size_of_headers as usize;
    let copy_len = headers_len.min(data.len());
    unsafe {
        core::ptr::copy_nonoverlapping(data.as_ptr(), mapping, copy_len);
    }
    if copy_len < headers_len {
        warn!(
            "PE headers truncated: size_of_headers=0x{:x} but file size=0x{:x}",
            headers_len,
            data.len()
        );
    }

    for section in file.sections() {
        if let Ok(data) = section.data() {
            // For PE/COFF, section.address() is the section RVA (VirtualAddress).
            let offset = section.address() as usize;
            info!(
                "Loading section {} to offset 0x{:x}, size 0x{:x}",
                section.name().unwrap_or("<unnamed>"),
                offset,
                data.len()
            );
            if offset.checked_add(data.len()).unwrap_or(usize::MAX) > meta.size_of_image as usize {
                warn!(
                    "Section {} out of bounds: offset=0x{:x} size=0x{:x} size_of_image=0x{:x}",
                    section.name().unwrap_or("<unnamed>"),
                    offset,
                    data.len(),
                    meta.size_of_image
                );
                continue;
            }
            unsafe {
                core::ptr::copy_nonoverlapping(data.as_ptr(), mapping.add(offset), data.len());
            }
        }
    }
}

pub fn apply_relocations(
    file: &File,
    mapping: *mut u8,
    loaded_image_base: u64,
    preferred_image_base: u64,
    size_of_image: u32,
) {
    let delta = loaded_image_base as i64 - preferred_image_base as i64;

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

                    // `va` is the Page RVA for this block; patch address is also an RVA.
                    let patch_rva = va as u64 + roffset as u64;
                    if patch_rva >= size_of_image as u64 {
                        warn!(
                            "Relocation RVA out of bounds: rva=0x{:x} size_of_image=0x{:x}",
                            patch_rva, size_of_image
                        );
                        continue;
                    }
                    let patch_offset = patch_rva as usize;

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
