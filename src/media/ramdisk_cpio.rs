use core::{ptr, str};
use axhal::mem::phys_to_virt;

const CPIO_MAGIC: &[u8; 6] = b"070701";
const CPIO_BASE: usize = 0x8400_0000;

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

pub fn parse_cpio_ramdisk() {
    let mut ptr = phys_to_virt(CPIO_BASE.into()).as_usize();

    loop {
        let hdr = unsafe { &*(ptr as *const CpioNewcHeader) };

        if &hdr.c_magic != CPIO_MAGIC {
            info!("Invalid magic at {:#x}", ptr);
            break;
        }

        let namesize = parse_hex_field(&hdr.c_namesize);
        let filesize = parse_hex_field(&hdr.c_filesize);

        // 文件名在 header 后面紧跟着
        let name_ptr = ptr + core::mem::size_of::<CpioNewcHeader>();
        let name = unsafe {
            let slice = core::slice::from_raw_parts(name_ptr as *const u8, namesize - 1); // 去掉末尾的 '\0'
            str::from_utf8(slice).unwrap_or("<invalid utf8>")
        };

        if name == "TRAILER!!!" {
            info!("End of CPIO archive.");
            break;
        }

        info!("Found file: {}, size: {}", name, filesize);

        // 文件数据
        let file_start = align_up(name_ptr + namesize, 4);
        let file_end = file_start + filesize;

        if name == "arceboot.txt" {
            let data = unsafe {
                core::slice::from_raw_parts(file_start as *const u8, filesize)
            };
            if let Ok(text) = core::str::from_utf8(data) {
                info!("Content of {}:\n{}", name, text);
            } else {
                info!("Binary content of {}: {:02x?}", name, &data[0..core::cmp::min(32, data.len())]);
            }
        }

        // 移动到下一个 header
        ptr = align_up(file_end, 4);
    }
}

fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}