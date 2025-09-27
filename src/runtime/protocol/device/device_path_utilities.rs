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
const DEVICE_PATH_HEADER_LENGTH: usize = 4;

static DEVICE_PATH_UTILITIES: LazyInit<Mutex<DevicePathUtilities>> = LazyInit::new();

#[inline]
unsafe fn get_node_type(protocol: *const DevicePathProtocol) -> u8 {
    unsafe { *(protocol as *const u8) }
}
#[inline]
unsafe fn get_node_subtype(protocol: *const DevicePathProtocol) -> u8 {
    unsafe { *((protocol as *const u8).add(1)) }
}
#[inline]
unsafe fn get_node_length_u16(protocol: *const DevicePathProtocol) -> u16 {
    // Little-endian 2 bytes at offset 2
    // equivalent to read_unaligned((b+2) as *const u16)
    unsafe { *(protocol as *const u16).add(1) }
}
#[inline]
unsafe fn get_node_length(protocol: *const DevicePathProtocol) -> usize {
    unsafe { get_node_length_u16(protocol) as usize }
}
#[inline]
unsafe fn is_end_node(protocol: *const DevicePathProtocol) -> bool {
    unsafe {
        get_node_type(protocol) == END_DEVICE_PATH_TYPE
            && get_node_length(protocol) >= DEVICE_PATH_HEADER_LENGTH
    }
}
#[inline]
unsafe fn is_end_entire_node(protocol: *const DevicePathProtocol) -> bool {
    unsafe { is_end_node(protocol) && get_node_subtype(protocol) == END_ENTIRE_DEVICE_PATH_SUBTYPE }
}
#[inline]
unsafe fn is_end_instance_node(protocol: *const DevicePathProtocol) -> bool {
    unsafe {
        is_end_node(protocol) && get_node_subtype(protocol) == END_INSTANCE_DEVICE_PATH_SUBTYPE
    }
}

#[inline]
unsafe fn compute_total_device_path_size(
    device_path_ptr: *const DevicePathProtocol,
) -> Option<usize> {
    if device_path_ptr.is_null() {
        return Some(0);
    }
    let mut current_ptr = device_path_ptr;
    let mut total_size: usize = 0;
    let hard_size_cap: usize = 1 << 20;

    loop {
        let node_length = unsafe { get_node_length(current_ptr) };
        if node_length < DEVICE_PATH_HEADER_LENGTH {
            return None;
        }
        total_size = total_size.checked_add(node_length)?;
        if total_size > hard_size_cap {
            return None;
        }
        if unsafe { is_end_entire_node(current_ptr) } {
            break;
        }
        current_ptr =
            unsafe { (current_ptr as *const u8).add(node_length) } as *const DevicePathProtocol;
    }
    Some(total_size)
}

#[inline]
unsafe fn copy_device_path_bytes_to_box(
    device_path_ptr: *const DevicePathProtocol,
    total_size: usize,
) -> *const DevicePathProtocol {
    if total_size == 0 {
        return ptr::null();
    }
    let source_slice = unsafe { slice::from_raw_parts(device_path_ptr as *const u8, total_size) };
    let mut buffer = Vec::<u8>::with_capacity(total_size);
    buffer.extend_from_slice(source_slice);
    let boxed_slice = buffer.into_boxed_slice();
    let raw_u8_ptr = Box::into_raw(boxed_slice) as *mut u8;
    raw_u8_ptr as *const DevicePathProtocol
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
        match compute_total_device_path_size(device_path) {
            Some(total_size) => total_size,
            None => 0,
        }
    }
}

pub extern "efiapi" fn duplicate_device_path(
    device_path: *const DevicePathProtocol,
) -> *const DevicePathProtocol {
    unsafe {
        match compute_total_device_path_size(device_path) {
            Some(0) => ptr::null_mut(),
            Some(total_size) => copy_device_path_bytes_to_box(device_path, total_size) as *const _,
            None => ptr::null_mut(),
        }
    }
}

pub extern "efiapi" fn append_device_path(
    first_path: *const DevicePathProtocol,
    second_path: *const DevicePathProtocol,
) -> *const DevicePathProtocol {
    unsafe {
        let first_total_size = match compute_total_device_path_size(first_path) {
            Some(0) => DEVICE_PATH_HEADER_LENGTH,
            Some(total_size) => total_size,
            None => return ptr::null_mut(),
        };
        let second_total_size = match compute_total_device_path_size(second_path) {
            Some(0) => DEVICE_PATH_HEADER_LENGTH,
            Some(total_size) => total_size,
            None => return ptr::null_mut(),
        };

        let first_without_end_length = if first_total_size >= DEVICE_PATH_HEADER_LENGTH {
            first_total_size - DEVICE_PATH_HEADER_LENGTH
        } else {
            0
        };
        let output_total_size = first_without_end_length
            .checked_add(second_total_size)
            .unwrap_or(0);
        if output_total_size == 0 {
            return ptr::null_mut();
        }

        let mut output_bytes = Vec::<u8>::with_capacity(output_total_size);
        if first_without_end_length > 0 && !first_path.is_null() {
            output_bytes.extend_from_slice(slice::from_raw_parts(
                first_path as *const u8,
                first_without_end_length,
            ));
        }
        if second_total_size > 0 && !second_path.is_null() {
            output_bytes.extend_from_slice(slice::from_raw_parts(
                second_path as *const u8,
                second_total_size,
            ));
        } else {
            // If second path is empty, terminate with End Entire node
            output_bytes.extend_from_slice(&[
                END_DEVICE_PATH_TYPE,
                END_ENTIRE_DEVICE_PATH_SUBTYPE,
                0x04,
                0x00,
            ]);
        }

        let boxed = output_bytes.into_boxed_slice();
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
        let device_node_length = get_node_length(device_node);
        if device_node_length < DEVICE_PATH_HEADER_LENGTH || is_end_node(device_node) {
            return duplicate_device_path(device_path);
        }

        let device_path_total_size = match compute_total_device_path_size(device_path) {
            Some(0) => DEVICE_PATH_HEADER_LENGTH,
            Some(total_size) => total_size,
            None => return ptr::null_mut(),
        };

        let device_path_without_end_length = device_path_total_size - DEVICE_PATH_HEADER_LENGTH;
        let output_total_size = device_path_without_end_length
            .checked_add(device_node_length)
            .and_then(|v| v.checked_add(DEVICE_PATH_HEADER_LENGTH))
            .unwrap_or(0);
        if output_total_size == 0 {
            return ptr::null_mut();
        }

        let mut output_bytes = Vec::<u8>::with_capacity(output_total_size);
        if device_path_without_end_length > 0 && !device_path.is_null() {
            output_bytes.extend_from_slice(slice::from_raw_parts(
                device_path as *const u8,
                device_path_without_end_length,
            ));
        }
        output_bytes.extend_from_slice(slice::from_raw_parts(
            device_node as *const u8,
            device_node_length,
        ));
        // Append End Entire node
        output_bytes.extend_from_slice(&[
            END_DEVICE_PATH_TYPE,
            END_ENTIRE_DEVICE_PATH_SUBTYPE,
            0x04,
            0x00,
        ]);

        let boxed = output_bytes.into_boxed_slice();
        Box::into_raw(boxed) as *const DevicePathProtocol
    }
}

pub extern "efiapi" fn append_device_path_instance(
    device_path: *const DevicePathProtocol,
    device_path_instance: *const DevicePathProtocol,
) -> *const DevicePathProtocol {
    unsafe {
        let device_path_total_size = match compute_total_device_path_size(device_path) {
            Some(0) => DEVICE_PATH_HEADER_LENGTH,
            Some(total_size) => total_size,
            None => return ptr::null_mut(),
        };
        let device_path_without_end_length = device_path_total_size - DEVICE_PATH_HEADER_LENGTH;
        if device_path_instance.is_null() {
            return duplicate_device_path(device_path);
        }

        let mut current_ptr = device_path_instance;
        let mut instance_total_length: usize = 0;
        loop {
            let node_length = get_node_length(current_ptr);
            if node_length < DEVICE_PATH_HEADER_LENGTH {
                return ptr::null_mut();
            }
            instance_total_length = instance_total_length.checked_add(node_length).unwrap_or(0);
            if is_end_instance_node(current_ptr) || is_end_entire_node(current_ptr) {
                break;
            }
            current_ptr = (current_ptr as *const u8).add(node_length) as *const DevicePathProtocol;
        }

        // output = device_path_without_end_length + instance_total_length + End Entire (4 bytes)
        let output_total_size = device_path_without_end_length
            .checked_add(instance_total_length)
            .and_then(|v| v.checked_add(DEVICE_PATH_HEADER_LENGTH))
            .unwrap_or(0);
        if output_total_size == 0 {
            return ptr::null_mut();
        }

        let mut output_bytes = Vec::<u8>::with_capacity(output_total_size);
        if device_path_without_end_length > 0 && !device_path.is_null() {
            output_bytes.extend_from_slice(slice::from_raw_parts(
                device_path as *const u8,
                device_path_without_end_length,
            ));
            // Insert an End Instance node between instances
            output_bytes.extend_from_slice(&[
                END_DEVICE_PATH_TYPE,
                END_INSTANCE_DEVICE_PATH_SUBTYPE,
                0x04,
                0x00,
            ]);
        }
        output_bytes.extend_from_slice(slice::from_raw_parts(
            device_path_instance as *const u8,
            instance_total_length,
        ));
        // Remove the last 4 bytes of the copied instance (its End node), then add End Entire
        if let Some(_) =
            output_bytes.get(output_bytes.len().saturating_sub(DEVICE_PATH_HEADER_LENGTH)..)
        {
            output_bytes.truncate(output_bytes.len().saturating_sub(DEVICE_PATH_HEADER_LENGTH));
        }
        output_bytes.extend_from_slice(&[
            END_DEVICE_PATH_TYPE,
            END_ENTIRE_DEVICE_PATH_SUBTYPE,
            0x04,
            0x00,
        ]);

        let boxed = output_bytes.into_boxed_slice();
        Box::into_raw(boxed) as *const DevicePathProtocol
    }
}

pub extern "efiapi" fn get_next_device_path_instance(
    device_path_instance_ptr: *mut *const DevicePathProtocol,
    device_path_instance_size_out: *mut usize,
) -> *const DevicePathProtocol {
    unsafe {
        if device_path_instance_ptr.is_null() || device_path_instance_size_out.is_null() {
            return ptr::null_mut();
        }
        let current_instance_ptr = *device_path_instance_ptr;
        if current_instance_ptr.is_null() {
            *device_path_instance_size_out = 0;
            return ptr::null_mut();
        }

        let start_bytes_ptr = current_instance_ptr as *const u8;
        let mut cursor_ptr = current_instance_ptr;
        let mut instance_bytes_length: usize = 0;
        loop {
            let node_length = get_node_length(cursor_ptr);
            if node_length < DEVICE_PATH_HEADER_LENGTH {
                return ptr::null_mut();
            }
            instance_bytes_length = instance_bytes_length.checked_add(node_length).unwrap_or(0);
            if is_end_instance_node(cursor_ptr) || is_end_entire_node(cursor_ptr) {
                break;
            }
            cursor_ptr = (cursor_ptr as *const u8).add(node_length) as *const DevicePathProtocol;
        }

        let mut output_bytes = Vec::<u8>::with_capacity(instance_bytes_length);
        output_bytes.extend_from_slice(slice::from_raw_parts(
            start_bytes_ptr,
            instance_bytes_length,
        ));
        if output_bytes.len() >= DEVICE_PATH_HEADER_LENGTH {
            output_bytes.truncate(output_bytes.len() - DEVICE_PATH_HEADER_LENGTH);
        }
        output_bytes.extend_from_slice(&[
            END_DEVICE_PATH_TYPE,
            END_ENTIRE_DEVICE_PATH_SUBTYPE,
            0x04,
            0x00,
        ]);

        let after_current_instance = (cursor_ptr as *const u8).add(DEVICE_PATH_HEADER_LENGTH);
        if is_end_instance_node(cursor_ptr) {
            *device_path_instance_ptr = after_current_instance as *const DevicePathProtocol;
        } else {
            *device_path_instance_ptr = ptr::null();
        }

        *device_path_instance_size_out = output_bytes.len();
        let boxed = output_bytes.into_boxed_slice();
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
        let mut cursor_ptr = device_path;
        loop {
            if is_end_instance_node(cursor_ptr) {
                return true;
            }
            if is_end_entire_node(cursor_ptr) {
                return false;
            }
            let node_length = get_node_length(cursor_ptr);
            if node_length < DEVICE_PATH_HEADER_LENGTH {
                return false;
            }
            cursor_ptr = (cursor_ptr as *const u8).add(node_length) as *const DevicePathProtocol;
        }
    }
}

pub extern "efiapi" fn create_device_node(
    node_type: DeviceType,
    node_sub_type: DeviceSubType,
    node_length: u16,
) -> *const DevicePathProtocol {
    unsafe {
        if usize::from(node_length) < DEVICE_PATH_HEADER_LENGTH {
            return ptr::null_mut();
        }
        let mut buffer = Vec::<u8>::with_capacity(node_length as usize);
        buffer.resize(node_length as usize, 0u8);

        buffer[0] = mem::transmute::<DeviceType, u8>(node_type);
        buffer[1] = mem::transmute::<DeviceSubType, u8>(node_sub_type);
        buffer[2] = (node_length & 0x00FF) as u8;
        buffer[3] = (node_length >> 8) as u8;

        let boxed = buffer.into_boxed_slice();
        Box::into_raw(boxed) as *const DevicePathProtocol
    }
}
