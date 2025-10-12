use axsync::Mutex;
use lazyinit::LazyInit;
use uefi_raw::{
    Char16,
    protocol::device_path::{DevicePathFromTextProtocol, DevicePathProtocol},
};

use alloc::boxed::Box;

static DEVICE_PATH_FROM_TEXT: LazyInit<Mutex<DevicePathFromText>> = LazyInit::new();

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

pub fn init_device_path_from_text() {
    DEVICE_PATH_FROM_TEXT.init_once(Mutex::new(DevicePathFromText::new()));
}

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
