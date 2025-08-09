use core::ffi::c_void;

use axsync::Mutex;
use lazyinit::LazyInit;
use uefi_raw::{
    Char16, Guid, Status,
    protocol::file_system::{
        FileAttribute, FileIoToken, FileMode, FileProtocolRevision, FileProtocolV1, FileProtocolV2,
        SimpleFileSystemProtocol,
    },
};

use alloc::boxed::Box;

static TEXT_FILE_SYSTEM: LazyInit<Mutex<SimpleFileSystem>> = LazyInit::new();

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

pub extern "efiapi" fn open_volume(
    _this: *mut SimpleFileSystemProtocol,
    _root: *mut *mut FileProtocolV1,
) -> Status {
    Status::UNSUPPORTED
}

#[derive(Debug)]
pub struct FileProtocolV1Impl {
    protocol: &'static mut FileProtocolV1,
    protocol_raw: *mut FileProtocolV1,
}

impl FileProtocolV1Impl {
    pub fn new() -> Self {
        let protocol = FileProtocolV1 {
            revision: FileProtocolRevision::REVISION_1,
            open,
            close,
            delete,
            read,
            write,
            get_position,
            set_position,
            get_info,
            set_info,
            flush,
        };
        let protocol_raw = Box::into_raw(Box::new(protocol));
        let protocol = unsafe { &mut *protocol_raw };
        Self {
            protocol,
            protocol_raw,
        }
    }

    pub fn get_protocol(&self) -> *mut FileProtocolV1 {
        self.protocol_raw
    }
}

unsafe impl Send for FileProtocolV1Impl {}
unsafe impl Sync for FileProtocolV1Impl {}

#[derive(Debug)]
pub struct FileProtocolV2Impl {
    protocol: &'static mut FileProtocolV2,
    protocol_raw: *mut FileProtocolV2,
}

impl FileProtocolV2Impl {
    pub fn new() -> Self {
        let v1 = FileProtocolV1Impl::new(); // reuse V1 implementation
        let protocol = FileProtocolV2 {
            v1: unsafe { *Box::from_raw(v1.get_protocol()) }, // clone contents, not share pointer
            open_ex,
            read_ex,
            write_ex,
            flush_ex,
        };
        let protocol_raw = Box::into_raw(Box::new(protocol));
        let protocol = unsafe { &mut *protocol_raw };
        Self {
            protocol,
            protocol_raw,
        }
    }

    pub fn get_protocol(&self) -> *mut FileProtocolV2 {
        self.protocol_raw
    }
}

unsafe impl Send for FileProtocolV2Impl {}
unsafe impl Sync for FileProtocolV2Impl {}

pub extern "efiapi" fn open(
    _this: *mut FileProtocolV1,
    _new_handle: *mut *mut FileProtocolV1,
    _file_name: *const Char16,
    _open_mode: FileMode,
    _attributes: FileAttribute,
) -> Status {
    Status::UNSUPPORTED
}

pub extern "efiapi" fn close(_this: *mut FileProtocolV1) -> Status {
    Status::UNSUPPORTED
}

pub extern "efiapi" fn delete(_this: *mut FileProtocolV1) -> Status {
    Status::UNSUPPORTED
}

pub extern "efiapi" fn read(
    _this: *mut FileProtocolV1,
    _buffer_size: *mut usize,
    _buffer: *mut c_void,
) -> Status {
    Status::UNSUPPORTED
}

pub extern "efiapi" fn write(
    _this: *mut FileProtocolV1,
    _buffer_size: *mut usize,
    _buffer: *const c_void,
) -> Status {
    Status::UNSUPPORTED
}

pub extern "efiapi" fn get_position(_this: *const FileProtocolV1, _position: *mut u64) -> Status {
    Status::UNSUPPORTED
}

pub extern "efiapi" fn set_position(_this: *mut FileProtocolV1, _position: u64) -> Status {
    Status::UNSUPPORTED
}

pub extern "efiapi" fn get_info(
    _this: *mut FileProtocolV1,
    _information_type: *const Guid,
    _buffer_size: *mut usize,
    _buffer: *mut c_void,
) -> Status {
    Status::UNSUPPORTED
}

pub extern "efiapi" fn set_info(
    _this: *mut FileProtocolV1,
    _information_type: *const Guid,
    _buffer_size: usize,
    _buffer: *const c_void,
) -> Status {
    Status::UNSUPPORTED
}

pub extern "efiapi" fn flush(_this: *mut FileProtocolV1) -> Status {
    Status::UNSUPPORTED
}

// v2-specific
pub extern "efiapi" fn open_ex(
    _this: *mut FileProtocolV2,
    _new_handle: *mut *mut FileProtocolV2,
    _file_name: *const Char16,
    _open_mode: FileMode,
    _attributes: FileAttribute,
    _token: *mut FileIoToken,
) -> Status {
    Status::UNSUPPORTED
}

pub extern "efiapi" fn read_ex(_this: *mut FileProtocolV2, _token: *mut FileIoToken) -> Status {
    Status::UNSUPPORTED
}

pub extern "efiapi" fn write_ex(_this: *mut FileProtocolV2, _token: *mut FileIoToken) -> Status {
    Status::UNSUPPORTED
}

pub extern "efiapi" fn flush_ex(_this: *mut FileProtocolV2, _token: *mut FileIoToken) -> Status {
    Status::UNSUPPORTED
}
