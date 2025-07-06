use core::{mem::transmute, ptr::{copy_nonoverlapping, null_mut}};

use axhal::{mem::{MemoryAddr, VirtAddr}, paging::MappingFlags};
use object::Object;
use object::File;
use object::ObjectSection;

use crate::platform::{efi::EfiTableHeader, protocol::output::EfiSimpleTextOutputProtocol, table::EfiSystemTable};

extern crate alloc;
use alloc::boxed::Box;

pub mod efi;
pub mod protocol;
pub mod table;

static SHELLCODE: &[u8] = include_bytes!("../../myramdisk/HelloRiscv.efi");

pub type EfiMainFn = extern "efiapi" fn(
    image_handle: *mut core::ffi::c_void,
    system_table: *mut EfiSystemTable,
) -> u64;

extern "efiapi" fn mock_output_string(
    _this: *mut EfiSimpleTextOutputProtocol,
    string: *const u16,
) -> u64 {
    info!("EFI Output Called");
    42
}

pub fn efi_runtime_init() {
    let file = File::parse(&*SHELLCODE).unwrap();
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
    let layout: core::alloc::Layout = core::alloc::Layout::from_size_align(
        SHELLCODE.len(),
        4096,
    ).expect("Invalid layout for shellcode");

    let mapping = axalloc::global_allocator().alloc(layout).expect("Failed to allocate memory for shellcode");
    let mapping = mapping.as_ptr();

    axmm::kernel_aspace()
        .lock()
        .protect(VirtAddr::from_ptr_of(mapping).align_down(4096usize), page_count * 4096, flags)
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

    let image_handle: *mut core::ffi::c_void = null_mut();
    let system_table = Box::into_raw(Box::new(EfiSystemTable {
        hdr: EfiTableHeader {
            signature: 0,
            revision: 0,
            header_size: 0,
            crc32: 0,
            reserved: 0,
        },
        firmware_vendor: null_mut(),
        firmware_revision: 0,
        console_in_handle: null_mut(),
        con_in: 0,
        console_out_handle: null_mut(),
        con_out: Box::into_raw(Box::new(EfiSimpleTextOutputProtocol {
            reset: 0,
            output_string: mock_output_string,
            test_string: 0,
            query_mode: 0,
            set_mode: 0,
            set_attribute: 0,
            clear_screen: 0,
            set_cursor_position: 0,
            enable_cursor: 0,
            mode: 0,
        })),
        standard_error_handle: null_mut(),
        std_err: 0,
        runtime_services: 0,
        boot_services: 0,
        number_of_table_entries: 0,
        configuration_table: 0,
    }));

    let result = func(image_handle, system_table);
    info!("efi_main return: {}", result)
}