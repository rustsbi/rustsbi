use alloc::{string::String, string::ToString, vec::Vec};
use axhal::mem::{PhysAddr, VirtAddr, phys_to_virt, virt_to_phys};
use axio::{self as io};
use core::str;

const CPIO_MAGIC: &[u8; 6] = b"070701";
pub static mut CPIO_BASE: usize = 0x0;

#[repr(C)]
#[derive(Debug)]
struct CpioNewcHeader {
    c_magic: [u8; 6],
    c_ino: [u8; 8],
    c_mode: [u8; 8],
    c_uid: [u8; 8],
    c_gid: [u8; 8],
    c_nlink: [u8; 8],
    c_mtime: [u8; 8],
    c_filesize: [u8; 8],
    c_devmajor: [u8; 8],
    c_devminor: [u8; 8],
    c_rdevmajor: [u8; 8],
    c_rdevminor: [u8; 8],
    c_namesize: [u8; 8],
    c_check: [u8; 8],
}

fn parse_hex_field(field: &[u8]) -> usize {
    // 将ASCII十六进制转换为数字
    let s = core::str::from_utf8(field).unwrap_or("0");
    usize::from_str_radix(s, 16).unwrap_or(0)
}

fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}

/// Returns the current working directory as a [`String`].
pub fn current_dir() -> io::Result<String> {
    Ok("/".to_string())
}

/// Read the entire contents of a file into a bytes vector.
pub fn read(path: &str) -> io::Result<Vec<u8>> {
    if unsafe { CPIO_BASE } == 0 {
        error!("Ramdisk is not enabled!");
        return core::prelude::v1::Err(io::Error::Unsupported);
    }

    let mut ptr = phys_to_virt(unsafe { CPIO_BASE.into() }).as_usize();

    loop {
        let hdr = unsafe { &*(ptr as *const CpioNewcHeader) };

        if &hdr.c_magic != CPIO_MAGIC {
            break;
        }

        let namesize = parse_hex_field(&hdr.c_namesize);
        let filesize = parse_hex_field(&hdr.c_filesize);

        let name_ptr = ptr + core::mem::size_of::<CpioNewcHeader>();
        let name = unsafe {
            let slice = core::slice::from_raw_parts(name_ptr as *const u8, namesize - 1);
            str::from_utf8(slice).unwrap_or("<invalid utf8>")
        };

        if name == "TRAILER!!!" {
            break;
        }

        let file_start = align_up(name_ptr + namesize, 4);
        let file_end = file_start + filesize;

        let is_match = if path.starts_with('/') {
            &path[1..] == name
        } else {
            path == name
        };

        if is_match {
            let data = unsafe { core::slice::from_raw_parts(file_start as *const u8, filesize) };
            let mut bytes = Vec::with_capacity(filesize as usize);
            bytes.extend_from_slice(data);
            return Ok(bytes);
        }

        ptr = align_up(file_end, 4);
    }

    core::prelude::v1::Err(io::Error::NotFound)
}

/// Read the entire contents of a file into a string.
pub fn read_to_string(path: &str) -> io::Result<String> {
    if unsafe { CPIO_BASE } == 0 {
        error!("Ramdisk is not enabled!");
        return core::prelude::v1::Err(io::Error::Unsupported);
    }

    let mut ptr = phys_to_virt(unsafe { CPIO_BASE.into() }).as_usize();

    loop {
        let hdr = unsafe { &*(ptr as *const CpioNewcHeader) };

        if &hdr.c_magic != CPIO_MAGIC {
            break;
        }

        let namesize = parse_hex_field(&hdr.c_namesize);
        let filesize = parse_hex_field(&hdr.c_filesize);

        let name_ptr = ptr + core::mem::size_of::<CpioNewcHeader>();
        let name = unsafe {
            let slice = core::slice::from_raw_parts(name_ptr as *const u8, namesize - 1);
            str::from_utf8(slice).unwrap_or("<invalid utf8>")
        };

        if name == "TRAILER!!!" {
            break;
        }

        let file_start = align_up(name_ptr + namesize, 4);
        let file_end = file_start + filesize;

        let is_match = if path.starts_with('/') {
            &path[1..] == name
        } else {
            path == name
        };

        if is_match {
            let data = unsafe {
                let slice = core::slice::from_raw_parts(file_start as *const u8, filesize);
                str::from_utf8(slice).unwrap_or("<invalid utf8>")
            };
            return Ok(data.to_string());
        }

        ptr = align_up(file_end, 4);
    }

    core::prelude::v1::Err(io::Error::NotFound)
}

fn set_ramdisk_addr(addr: usize) {
    unsafe {
        CPIO_BASE = addr;
    }
}

fn ramdisk_enabled() -> bool {
    axconfig::boot::USE_RAMDISK
}

fn ramdisk_load_enabled() -> bool {
    axconfig::boot::LOAD_RAMDISK
}

fn do_load_ramdisk() -> Option<(usize, usize)> {
    let file_data = crate::medium::virtio_disk::read(axconfig::boot::RAMDISK_FILE).unwrap();
    let file_size = file_data.len();

    let start_addr = axalloc::global_allocator()
        .alloc_pages(file_size / 4096 + 1, 4096)
        .unwrap();
    unsafe {
        core::ptr::copy_nonoverlapping(file_data.as_ptr(), start_addr as *mut u8, file_size);
    }
    debug!(
        "Ramdisk will be load to {:#x}, size is {:#x}",
        start_addr, file_size
    );
    Some((start_addr, file_size))
}

fn enable_dtb_ramdisk(addr: usize, size: usize) {
    unsafe {
        let mut parser = crate::dtb::DtbParser::new(crate::dtb::GLOBAL_NOW_DTB_ADDRESS).unwrap();

        // linux initrd
        if !parser.add_property("/chosen", "linux,initrd-start", &addr.to_ne_bytes()) {
            error!("Change linux,initrd-start failed!");
        }
        if !parser.add_property(
            "/chosen",
            "linux,initrd-end",
            &((addr + size).to_ne_bytes()),
        ) {
            error!("Change linux,initrd-end failed!");
        }

        // Save new dtb
        crate::dtb::GLOBAL_NOW_DTB_ADDRESS = parser.save_to_mem();
    }
}

pub fn check_ramdisk() {
    info!("Checking ramdisk.....");
    // Check the detailed annotations and explanations in configs/platforms/riscv64-qemu-virt.toml
    if crate::medium::ramdisk_cpio::ramdisk_enabled() {
        let mut start_addr_phys: usize = 0;
        let mut _start_addr_virt: usize = 0;
        let mut size: usize = 0;
        if crate::medium::ramdisk_cpio::ramdisk_load_enabled() {
            if let Some((addr, si)) = crate::medium::ramdisk_cpio::do_load_ramdisk() {
                _start_addr_virt = addr;
                start_addr_phys = virt_to_phys(VirtAddr::from_usize(addr)).as_usize();
                size = si;
            } else {
                error!("Load ramdisk failed!");
            }
            crate::medium::ramdisk_cpio::set_ramdisk_addr(start_addr_phys);
            crate::medium::ramdisk_cpio::enable_dtb_ramdisk(start_addr_phys, size);
            info!(
                "read test file context: {}",
                crate::medium::ramdisk_cpio::read_to_string("/test/arceboot.txt").unwrap()
            );
        } else {
            start_addr_phys = axconfig::boot::RAMDISK_START;
            _start_addr_virt = phys_to_virt(PhysAddr::from_usize(start_addr_phys)).as_usize();
            size = axconfig::boot::RAMDISK_SIZE;

            crate::medium::ramdisk_cpio::set_ramdisk_addr(start_addr_phys);
            crate::medium::ramdisk_cpio::enable_dtb_ramdisk(start_addr_phys, size);
            info!(
                "read test file context: {}",
                crate::medium::ramdisk_cpio::read_to_string("/test/arceboot.txt").unwrap()
            );
        }
    }
    info!("Checking for ramdisk is done!");
}
