use axsync::Mutex;
use lazyinit::LazyInit;
use uefi_raw::{
    Boolean, Char16,
    protocol::device_path::{DevicePathProtocol, DevicePathToTextProtocol},
};

use alloc::boxed::Box;

static DEVICE_PATH_TO_TEXT: LazyInit<Mutex<DevicePathToText>> = LazyInit::new();

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

pub fn init_device_path_to_text() {
    DEVICE_PATH_TO_TEXT.init_once(Mutex::new(DevicePathToText::new()));
}

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
