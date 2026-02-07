use uefi_raw::{
    Char16, Status,
    protocol::file_system::{FileAttribute, FileIoToken, FileMode, FileProtocolV2},
};

use crate::runtime::protocol::fs::{HandleKind, file_protocol_v1::FileProtocolV1Impl};

use alloc::boxed::Box;

#[repr(C)]
#[derive(Debug)]
pub struct FileProtocolV2Impl {
    protocol: &'static mut FileProtocolV2,
    protocol_raw: *mut FileProtocolV2,
}

impl FileProtocolV2Impl {
    pub fn new(path: &str, mode: FileMode, kind: HandleKind) -> Self {
        let v1 = FileProtocolV1Impl::new(path, mode, kind); // reuse V1 implementation
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

// v2-specific. Refer to UEFI Spec 2.11 Section 13.5.7 - 13.5.10.
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
