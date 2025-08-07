use core::ptr::null_mut;

use axsync::Mutex;
use lazyinit::LazyInit;
use uefi_raw::{
    Boolean, Char16,
    protocol::device_path::{
        DevicePathFromTextProtocol, DevicePathProtocol, DevicePathToTextProtocol,
        DevicePathUtilitiesProtocol, DeviceSubType, DeviceType,
    },
};

extern crate alloc;
use alloc::boxed::Box;

static DEVICE_PATH_TO_TEXT: LazyInit<Mutex<DevicePathToText>> = LazyInit::new();
static DEVICE_PATH_FROM_TEXT: LazyInit<Mutex<DevicePathFromText>> = LazyInit::new();
static DEVICE_PATH_UTILITIES: LazyInit<Mutex<DevicePathUtilities>> = LazyInit::new();

#[derive(Debug)]
pub struct DevicePathToText {
    protocol: &'static mut DevicePathToTextProtocol,
    protocol_raw: *mut DevicePathToTextProtocol,
}

impl DevicePathToText {
    pub fn new() -> Self {
        let protocol = DevicePathToTextProtocol {
            convert_device_node_to_text,
            convert_device_path_to_text,
        };
        let protocol_raw = Box::into_raw(Box::new(protocol));
        let protocol = unsafe { &mut *protocol_raw };
        Self {
            protocol,
            protocol_raw,
        }
    }

    pub fn get_protocol(&self) -> *mut DevicePathToTextProtocol {
        self.protocol_raw
    }
}

unsafe impl Send for DevicePathToText {}
unsafe impl Sync for DevicePathToText {}

pub extern "efiapi" fn convert_device_node_to_text(
    _device_node: *const DevicePathProtocol,
    _display_only: Boolean,
    _allow_shortcuts: Boolean,
) -> *const Char16 {
    core::ptr::null()
}

pub extern "efiapi" fn convert_device_path_to_text(
    _device_path: *const DevicePathProtocol,
    _display_only: Boolean,
    _allow_shortcuts: Boolean,
) -> *const Char16 {
    core::ptr::null()
}

#[derive(Debug)]
pub struct DevicePathFromText {
    protocol: &'static mut DevicePathFromTextProtocol,
    protocol_raw: *mut DevicePathFromTextProtocol,
}

impl DevicePathFromText {
    pub fn new() -> Self {
        let protocol = DevicePathFromTextProtocol {
            convert_text_to_device_node,
            convert_text_to_device_path,
        };
        let protocol_raw = Box::into_raw(Box::new(protocol));
        let protocol = unsafe { &mut *protocol_raw };
        Self {
            protocol,
            protocol_raw,
        }
    }

    pub fn get_protocol(&self) -> *mut DevicePathFromTextProtocol {
        self.protocol_raw
    }
}

unsafe impl Send for DevicePathFromText {}
unsafe impl Sync for DevicePathFromText {}

pub extern "efiapi" fn convert_text_to_device_node(
    _text_device_node: *const Char16,
) -> *const DevicePathProtocol {
    core::ptr::null()
}

pub extern "efiapi" fn convert_text_to_device_path(
    _text_device_path: *const Char16,
) -> *const DevicePathProtocol {
    core::ptr::null()
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

pub extern "efiapi" fn get_device_path_size(_device_path: *const DevicePathProtocol) -> usize {
    0
}

pub extern "efiapi" fn duplicate_device_path(
    _device_path: *const DevicePathProtocol,
) -> *const DevicePathProtocol {
    null_mut()
}

pub extern "efiapi" fn append_device_path(
    _src1: *const DevicePathProtocol,
    _src2: *const DevicePathProtocol,
) -> *const DevicePathProtocol {
    null_mut()
}

pub extern "efiapi" fn append_device_node(
    _device_path: *const DevicePathProtocol,
    _device_node: *const DevicePathProtocol,
) -> *const DevicePathProtocol {
    null_mut()
}

pub extern "efiapi" fn append_device_path_instance(
    _device_path: *const DevicePathProtocol,
    _device_path_instance: *const DevicePathProtocol,
) -> *const DevicePathProtocol {
    null_mut()
}

pub extern "efiapi" fn get_next_device_path_instance(
    _device_path_instance: *mut *const DevicePathProtocol,
    _device_path_instance_size: *mut usize,
) -> *const DevicePathProtocol {
    null_mut()
}

pub extern "efiapi" fn is_device_path_multi_instance(
    _device_path: *const DevicePathProtocol,
) -> bool {
    true
}

pub extern "efiapi" fn create_device_node(
    _node_type: DeviceType,
    _node_sub_type: DeviceSubType,
    _node_length: u16,
) -> *const DevicePathProtocol {
    null_mut()
}
