use axsync::Mutex;
use lazyinit::LazyInit;
use uefi_raw::{
    Status,
    protocol::file_system::{FileProtocolV1, SimpleFileSystemProtocol},
};

use alloc::boxed::Box;

use crate::runtime::protocol::fs::file_protocol_v1::open_root;

static TEXT_FILE_SYSTEM: LazyInit<Mutex<SimpleFileSystem>> = LazyInit::new();

#[repr(C)]
#[derive(Debug)]
pub struct SimpleFileSystem {
    protocol: &'static mut SimpleFileSystemProtocol,
    protocol_raw: *mut SimpleFileSystemProtocol,
}

impl SimpleFileSystem {
    pub fn new() -> Self {
        let protocol = SimpleFileSystemProtocol {
            revision: 0x00010000,
            open_volume,
        };
        let protocol_raw = Box::into_raw(Box::new(protocol));
        let protocol = unsafe { &mut *protocol_raw };
        Self {
            protocol,
            protocol_raw,
        }
    }

    pub fn get_protocol(&self) -> *mut SimpleFileSystemProtocol {
        self.protocol_raw
    }
}

unsafe impl Send for SimpleFileSystem {}
unsafe impl Sync for SimpleFileSystem {}

pub fn init_simple_file_system() {
    TEXT_FILE_SYSTEM.init_once(Mutex::new(SimpleFileSystem::new()));
}

// impl SimpleFileSystem. Refer to UEFI Spec 2.11 Section 13.4.
pub extern "efiapi" fn open_volume(
    _this: *mut SimpleFileSystemProtocol,
    root: *mut *mut FileProtocolV1,
) -> Status {
    unsafe {
        *root = open_root();
    }

    Status::SUCCESS
}
