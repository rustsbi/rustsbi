extern crate alloc;

use core::{ptr, str};
use axhal::mem::phys_to_virt;
use axio::{self as io, prelude::*};
use alloc::{string::String, vec::Vec, string::ToString};

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

fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}

/// Returns the current working directory as a [`String`].
pub fn current_dir() -> io::Result<String> {
    Ok("/".to_string())
}

/// Read the entire contents of a file into a bytes vector.
pub fn read(path: &str) -> io::Result<Vec<u8>> {
    let mut ptr = phys_to_virt(CPIO_BASE.into()).as_usize();

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
                core::slice::from_raw_parts(file_start as *const u8, filesize)
            };
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
    let mut ptr = phys_to_virt(CPIO_BASE.into()).as_usize();

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
