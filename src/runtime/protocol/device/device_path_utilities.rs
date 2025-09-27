use core::{
    mem,
    ptr::{self},
    slice,
};

use axsync::Mutex;
use lazyinit::LazyInit;
use uefi_raw::protocol::device_path::{
    DevicePathProtocol, DevicePathUtilitiesProtocol, DeviceSubType, DeviceType,
};

use alloc::boxed::Box;
use alloc::vec::Vec;

// Device Path node type: 0x7F = End of Hardware Device Path
const END_DEVICE_PATH_TYPE: u8 = 0x7F;
// End node subtype: 0x01 = End Instance (marks end of one instance, more may follow)
const END_INSTANCE_DEVICE_PATH_SUBTYPE: u8 = 0x01;
// End node subtype: 0xFF = End Entire (marks end of the whole device path)
const END_ENTIRE_DEVICE_PATH_SUBTYPE: u8 = 0xFF;
// Common header size for every Device Path node (Type + SubType + Length[2])
const DEV_PATH_HEADER_LEN: usize = 4;

static DEVICE_PATH_UTILITIES: LazyInit<Mutex<DevicePathUtilities>> = LazyInit::new();

#[inline]
unsafe fn node_type(p: *const DevicePathProtocol) -> u8 {
    unsafe { *(p as *const u8) }
}
#[inline]
unsafe fn node_subtype(p: *const DevicePathProtocol) -> u8 {
    unsafe { *((p as *const u8).add(1)) }
}
#[inline]
unsafe fn node_len_u16(p: *const DevicePathProtocol) -> u16 {
    // Little-endian 2 bytes at offset 2
    // equivalent to read_unaligned((b+2) as *const u16)
    unsafe { *(p as *const u16).add(1) }
}
#[inline]
unsafe fn node_len(p: *const DevicePathProtocol) -> usize {
    unsafe { node_len_u16(p) as usize }
}
#[inline]
unsafe fn is_end(p: *const DevicePathProtocol) -> bool {
    unsafe { node_type(p) == END_DEVICE_PATH_TYPE && node_len(p) >= DEV_PATH_HEADER_LEN }
}
#[inline]
unsafe fn is_end_entire(p: *const DevicePathProtocol) -> bool {
    unsafe { is_end(p) && node_subtype(p) == END_ENTIRE_DEVICE_PATH_SUBTYPE }
}
#[inline]
unsafe fn is_end_instance(p: *const DevicePathProtocol) -> bool {
    unsafe { is_end(p) && node_subtype(p) == END_INSTANCE_DEVICE_PATH_SUBTYPE }
}

#[inline]
unsafe fn total_path_size(dp: *const DevicePathProtocol) -> Option<usize> {
    if dp.is_null() {
        return Some(0);
    }
    let mut cur = dp;
    let mut total: usize = 0;
    let hard_cap: usize = 1 << 20;

    loop {
        let len = unsafe { node_len(cur) };
        if len < DEV_PATH_HEADER_LEN {
            return None;
        }
        total = total.checked_add(len)?;
        if total > hard_cap {
            return None;
        }
        if unsafe { is_end_entire(cur) } {
            break;
        }
        cur = unsafe { (cur as *const u8).add(len) } as *const DevicePathProtocol;
    }
    Some(total)
}

#[inline]
unsafe fn copy_bytes_to_box(
    dp: *const DevicePathProtocol,
    size: usize,
) -> *const DevicePathProtocol {
    if size == 0 {
        return ptr::null();
    }
    let src = unsafe { slice::from_raw_parts(dp as *const u8, size) };
    let mut buf = Vec::<u8>::with_capacity(size);
    buf.extend_from_slice(src);
    let boxed = buf.into_boxed_slice();
    let ptr_u8 = Box::into_raw(boxed) as *mut u8;
    ptr_u8 as *const DevicePathProtocol
}

pub struct DevicePathUtilities {
    protocol: &'static mut DevicePathUtilitiesProtocol,
    protocol_raw: *mut DevicePathUtilitiesProtocol,
}

impl DevicePathUtilities {
    pub fn new() -> Self {
        let protocol = DevicePathUtilitiesProtocol {
            get_device_path_size,
            duplicate_device_path,
            append_device_path,
            append_device_node,
            append_device_path_instance,
            get_next_device_path_instance,
            is_device_path_multi_instance,
            create_device_node,
        };
        let protocol_raw = Box::into_raw(Box::new(protocol));
        let protocol = unsafe { &mut *protocol_raw };
        DevicePathUtilities {
            protocol,
            protocol_raw,
        }
    }

    pub fn get_protocol(&self) -> *mut DevicePathUtilitiesProtocol {
        let guard = DEVICE_PATH_UTILITIES.lock();
        guard.protocol_raw
    }
}

unsafe impl Send for DevicePathUtilities {}
unsafe impl Sync for DevicePathUtilities {}

pub fn init_device_path_uttilities() {
    DEVICE_PATH_UTILITIES.init_once(Mutex::new(DevicePathUtilities::new()));
}

pub extern "efiapi" fn get_device_path_size(device_path: *const DevicePathProtocol) -> usize {
    unsafe {
        match total_path_size(device_path) {
            Some(sz) => sz,
            None => 0,
        }
    }
}

pub extern "efiapi" fn duplicate_device_path(
    device_path: *const DevicePathProtocol,
) -> *const DevicePathProtocol {
    unsafe {
        match total_path_size(device_path) {
            Some(0) => ptr::null_mut(),
            Some(sz) => copy_bytes_to_box(device_path, sz) as *const _,
            None => ptr::null_mut(),
        }
    }
}

pub extern "efiapi" fn append_device_path(
    src1: *const DevicePathProtocol,
    src2: *const DevicePathProtocol,
) -> *const DevicePathProtocol {
    unsafe {
        let s1_size = match total_path_size(src1) {
            Some(0) => DEV_PATH_HEADER_LEN,
            Some(sz) => sz,
            None => return ptr::null_mut(),
        };
        let s2_size = match total_path_size(src2) {
            Some(0) => DEV_PATH_HEADER_LEN,
            Some(sz) => sz,
            None => return ptr::null_mut(),
        };

        let s1_head = if s1_size >= DEV_PATH_HEADER_LEN {
            s1_size - DEV_PATH_HEADER_LEN
        } else {
            0
        };
        let out_size = s1_head.checked_add(s2_size).unwrap_or(0);
        if out_size == 0 {
            return ptr::null_mut();
        }

        let mut out = Vec::<u8>::with_capacity(out_size);
        if s1_head > 0 && !src1.is_null() {
            out.extend_from_slice(slice::from_raw_parts(src1 as *const u8, s1_head));
        } else {
        }
        if s2_size > 0 && !src2.is_null() {
            out.extend_from_slice(slice::from_raw_parts(src2 as *const u8, s2_size));
        } else {
            out.extend_from_slice(&[
                END_DEVICE_PATH_TYPE,
                END_ENTIRE_DEVICE_PATH_SUBTYPE,
                0x04,
                0x00,
            ]);
        }

        let boxed = out.into_boxed_slice();
        Box::into_raw(boxed) as *const DevicePathProtocol
    }
}

pub extern "efiapi" fn append_device_node(
    device_path: *const DevicePathProtocol,
    device_node: *const DevicePathProtocol,
) -> *const DevicePathProtocol {
    unsafe {
        if device_node.is_null() {
            return duplicate_device_path(device_path);
        }
        let node_len_b = node_len(device_node);
        if node_len_b < DEV_PATH_HEADER_LEN || is_end(device_node) {
            return duplicate_device_path(device_path);
        }

        let dp_size = match total_path_size(device_path) {
            Some(0) => DEV_PATH_HEADER_LEN,
            Some(sz) => sz,
            None => return ptr::null_mut(),
        };

        let dp_head = dp_size - DEV_PATH_HEADER_LEN;
        let out_size = dp_head
            .checked_add(node_len_b)
            .and_then(|v| v.checked_add(DEV_PATH_HEADER_LEN))
            .unwrap_or(0);
        if out_size == 0 {
            return ptr::null_mut();
        }

        let mut out = Vec::<u8>::with_capacity(out_size);
        if dp_head > 0 && !device_path.is_null() {
            out.extend_from_slice(slice::from_raw_parts(device_path as *const u8, dp_head));
        }
        out.extend_from_slice(slice::from_raw_parts(device_node as *const u8, node_len_b));
        // End Entire
        out.extend_from_slice(&[
            END_DEVICE_PATH_TYPE,
            END_ENTIRE_DEVICE_PATH_SUBTYPE,
            0x04,
            0x00,
        ]);

        let boxed = out.into_boxed_slice();
        Box::into_raw(boxed) as *const DevicePathProtocol
    }
}

pub extern "efiapi" fn append_device_path_instance(
    device_path: *const DevicePathProtocol,
    device_path_instance: *const DevicePathProtocol,
) -> *const DevicePathProtocol {
    unsafe {
        let dp_size = match total_path_size(device_path) {
            Some(0) => DEV_PATH_HEADER_LEN,
            Some(sz) => sz,
            None => return ptr::null_mut(),
        };
        let dp_head = dp_size - DEV_PATH_HEADER_LEN;
        if device_path_instance.is_null() {
            return duplicate_device_path(device_path);
        }

        let mut cur = device_path_instance;
        let mut inst_len: usize = 0;
        loop {
            let len = node_len(cur);
            if len < DEV_PATH_HEADER_LEN {
                return ptr::null_mut();
            }
            inst_len = inst_len.checked_add(len).unwrap_or(0);
            if is_end_instance(cur) || is_end_entire(cur) {
                break;
            }
            cur = (cur as *const u8).add(len) as *const DevicePathProtocol;
        }

        // out = dp_head + inst_len + (End Entire 4B)
        let out_size = dp_head
            .checked_add(inst_len)
            .and_then(|v| v.checked_add(DEV_PATH_HEADER_LEN))
            .unwrap_or(0);
        if out_size == 0 {
            return ptr::null_mut();
        }

        let mut out = Vec::<u8>::with_capacity(out_size);
        if dp_head > 0 && !device_path.is_null() {
            out.extend_from_slice(slice::from_raw_parts(device_path as *const u8, dp_head));
            out.extend_from_slice(&[
                END_DEVICE_PATH_TYPE,
                END_INSTANCE_DEVICE_PATH_SUBTYPE,
                0x04,
                0x00,
            ]);
        }
        out.extend_from_slice(slice::from_raw_parts(
            device_path_instance as *const u8,
            inst_len,
        ));
        if let Some(_last4) = out.get(out.len().saturating_sub(DEV_PATH_HEADER_LEN)..) {
            out.truncate(out.len().saturating_sub(DEV_PATH_HEADER_LEN));
        }
        out.extend_from_slice(&[
            END_DEVICE_PATH_TYPE,
            END_ENTIRE_DEVICE_PATH_SUBTYPE,
            0x04,
            0x00,
        ]);

        let boxed = out.into_boxed_slice();
        Box::into_raw(boxed) as *const DevicePathProtocol
    }
}

pub extern "efiapi" fn get_next_device_path_instance(
    device_path_instance: *mut *const DevicePathProtocol,
    device_path_instance_size: *mut usize,
) -> *const DevicePathProtocol {
    unsafe {
        if device_path_instance.is_null() || device_path_instance_size.is_null() {
            return ptr::null_mut();
        }
        let cur = *device_path_instance;
        if cur.is_null() {
            *device_path_instance_size = 0;
            return ptr::null_mut();
        }

        let start = cur as *const u8;
        let mut p = cur;
        let mut inst_bytes: usize = 0;
        loop {
            let len = node_len(p);
            if len < DEV_PATH_HEADER_LEN {
                return ptr::null_mut();
            }
            inst_bytes = inst_bytes.checked_add(len).unwrap_or(0);
            if is_end_instance(p) || is_end_entire(p) {
                break;
            }
            p = (p as *const u8).add(len) as *const DevicePathProtocol;
        }

        let mut out = Vec::<u8>::with_capacity(inst_bytes);
        out.extend_from_slice(slice::from_raw_parts(start, inst_bytes));
        if out.len() >= DEV_PATH_HEADER_LEN {
            out.truncate(out.len() - DEV_PATH_HEADER_LEN);
        }
        out.extend_from_slice(&[
            END_DEVICE_PATH_TYPE,
            END_ENTIRE_DEVICE_PATH_SUBTYPE,
            0x04,
            0x00,
        ]);

        let after = (p as *const u8).add(DEV_PATH_HEADER_LEN);
        if is_end_instance(p) {
            *device_path_instance = after as *const DevicePathProtocol;
        } else {
            *device_path_instance = ptr::null();
        }

        *device_path_instance_size = out.len();
        let boxed = out.into_boxed_slice();
        Box::into_raw(boxed) as *const DevicePathProtocol
    }
}

pub extern "efiapi" fn is_device_path_multi_instance(
    device_path: *const DevicePathProtocol,
) -> bool {
    unsafe {
        if device_path.is_null() {
            return false;
        }
        let mut p = device_path;
        loop {
            if is_end_instance(p) {
                return true;
            }
            if is_end_entire(p) {
                return false;
            }
            let len = node_len(p);
            if len < DEV_PATH_HEADER_LEN {
                return false;
            }
            p = (p as *const u8).add(len) as *const DevicePathProtocol;
        }
    }
}

pub extern "efiapi" fn create_device_node(
    node_type: DeviceType,
    node_sub_type: DeviceSubType,
    node_length: u16,
) -> *const DevicePathProtocol {
    unsafe {
        if usize::from(node_length) < DEV_PATH_HEADER_LEN {
            return ptr::null_mut();
        }
        let mut buf = Vec::<u8>::with_capacity(node_length as usize);
        buf.resize(node_length as usize, 0u8);

        buf[0] = mem::transmute::<DeviceType, u8>(node_type);
        buf[1] = mem::transmute::<DeviceSubType, u8>(node_sub_type);
        buf[2] = (node_length & 0x00FF) as u8;
        buf[3] = (node_length >> 8) as u8;

        let boxed = buf.into_boxed_slice();
        Box::into_raw(boxed) as *const DevicePathProtocol
    }
}
