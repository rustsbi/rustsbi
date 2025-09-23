use core::ffi::c_void;

use axsync::Mutex;
use lazyinit::LazyInit;
use uefi_raw::{
    Char16, Guid, Status,
    protocol::file_system::{FileAttribute, FileMode, FileProtocolRevision, FileProtocolV1},
};

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

use crate::runtime::{
    protocol::fs::HandleKind,
    utils::{normalize_uefi_path, utf16_cstr_to_string},
};

#[derive(Debug)]
pub struct FileHandlerCounter {
    path: String,
    count: usize,
    file_handle: FileProtocolV1Impl,
}

#[allow(dead_code)]
#[derive(Clone)]
struct DirEnt {
    name: String,
    is_dir: bool,
    size: u64,
    create_time: u64,
    access_time: u64,
    modify_time: u64,
}

static FILE_HANDLER_MAP: LazyInit<Mutex<BTreeMap<&'static str, FileHandlerCounter>>> =
    LazyInit::new();

#[repr(C)]
#[derive(Debug)]
pub struct FileProtocolV1Impl {
    protocol: &'static mut FileProtocolV1,
    protocol_raw: *mut FileProtocolV1,
    path: String,
    mode: FileMode,
    kind: HandleKind,
    position: usize, // current read/write position for files
}

impl FileProtocolV1Impl {
    pub fn new(path: &str, mode: FileMode, kind: HandleKind) -> Self {
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
            path: path.to_string(),
            mode,
            kind,
            position: 0,
        }
    }

    pub fn get_protocol(&self) -> *mut FileProtocolV1 {
        self.protocol_raw
    }
}

unsafe impl Send for FileProtocolV1Impl {}
unsafe impl Sync for FileProtocolV1Impl {}

#[inline]
unsafe fn from_this(this: *mut FileProtocolV1) -> *mut FileProtocolV1Impl {
    this as *mut FileProtocolV1Impl
}

pub fn open_root() -> *mut FileProtocolV1 {
    // The OpenVolume() function opens a volume, and returns a file handle to the volume’s root directory.
    // This handle is used to perform all other file I/O operations.
    // The volume remains open until all the file handles to it are closed.
    let mut mapper = FILE_HANDLER_MAP
        .get()
        .expect("Failed to get FILE_HANDLER_MAP")
        .lock();

    if let Some(counter_box) = mapper.get_mut("/") {
        counter_box.count += 1;
        return counter_box.file_handle.get_protocol();
    }

    let file_handle =
        FileProtocolV1Impl::new("/", FileMode::READ | FileMode::WRITE, HandleKind::Dir);
    let counter = FileHandlerCounter {
        path: "/".to_string(),
        file_handle: file_handle,
        count: 1,
    };

    mapper.insert("/", counter);
    let counter_box = mapper.get_mut("/").expect("inserted but missing");

    return counter_box.file_handle.get_protocol();
}

// impl FileProtocolV1. Refer to UEFI Spec 2.11 Section 13.5.1.
pub extern "efiapi" fn open(
    this: *mut FileProtocolV1,
    new_handle: *mut *mut FileProtocolV1,
    file_name: *const Char16,
    open_mode: FileMode,
    attributes: FileAttribute,
) -> Status {
    if this.is_null() || new_handle.is_null() || file_name.is_null() {
        return Status::INVALID_PARAMETER;
    }

    let r = open_mode.contains(FileMode::READ);
    let w = open_mode.contains(FileMode::WRITE);
    let c = open_mode.contains(FileMode::CREATE);
    match (r, w, c) {
        (true, false, false) | (true, true, false) | (true, true, true) => {}
        _ => return Status::INVALID_PARAMETER,
    }

    let base = unsafe { &*(this as *mut FileProtocolV1Impl) }.path.clone();
    let name = unsafe { utf16_cstr_to_string(file_name) }.unwrap_or_default();
    let resolved = if name.is_empty() || name == "." {
        base
    } else {
        normalize_uefi_path(&base, &name)
    };

    // Read the actual existence and type (Some(true)=directory, Some(false)=file, None=does not exist)
    let (exists, exists_is_dir) = match axfs::api::metadata(&resolved) {
        Ok(md) => (true, md.is_dir()),
        Err(_) => (false, false),
    };

    // CREATE checks attributes and uses the DIRECTORY bit to determine the "expected type"
    let want_dir = if c {
        const VALID: u32 = (FileAttribute::READ_ONLY.bits()
            | FileAttribute::HIDDEN.bits()
            | FileAttribute::SYSTEM.bits()
            | FileAttribute::DIRECTORY.bits()
            | FileAttribute::ARCHIVE.bits()) as u32;
        if (attributes.bits() as u32) & !VALID != 0 {
            return Status::INVALID_PARAMETER;
        }
        attributes.contains(FileAttribute::DIRECTORY)
    } else {
        false
    };

    // 1. If CREATE is requested (c = true):
    //    - If the target already exists: we must check whether the actual type
    //      (exists_is_dir) matches the requested type (want_dir).
    //        * If they mismatch → return ACCESS_DENIED.
    //        * If they match → just open the existing object (need_create = false).
    //    - If the target does not exist: create a new object based on want_dir
    //      (directory vs file) (need_create = true).
    //
    // 2. If CREATE is not requested (c = false):
    //    - The target must already exist, otherwise return NOT_FOUND.
    //    - If it exists, simply use its actual type (final_is_dir = exists_is_dir).
    //
    // This logic boils down to two outputs:
    //    - final_is_dir: whether the resulting handle should represent a directory or a file
    //    - need_create : whether we must create a new object in the filesystem
    let (final_is_dir, need_create) = if c {
        if exists {
            if exists_is_dir != want_dir {
                return Status::ACCESS_DENIED;
            }
            (exists_is_dir, false)
        } else {
            (want_dir, true)
        }
    } else {
        if !exists {
            return Status::NOT_FOUND;
        }
        (exists_is_dir, false)
    };

    if need_create {
        let res = if final_is_dir {
            axfs::api::create_dir(&resolved)
        } else {
            axfs::api::write(&resolved, &[])
        };
        if let Err(_e) = res {
            return Status::ACCESS_DENIED;
        }
    }

    let handle = FileProtocolV1Impl::new(
        &resolved,
        open_mode,
        if final_is_dir {
            HandleKind::Dir
        } else {
            HandleKind::File
        },
    );
    unsafe {
        *new_handle = handle.get_protocol();
    }

    Status::SUCCESS
}

pub extern "efiapi" fn close(this: *mut FileProtocolV1) -> Status {
    if this.is_null() {
        return Status::INVALID_PARAMETER;
    }

    if unsafe { &mut *from_this(this) }.path == "/" {
        // Root directory is special: never remove it, just decrement count
        let mut mapper = FILE_HANDLER_MAP
            .get()
            .expect("Failed to get FILE_HANDLER_MAP")
            .lock();
        if let Some(counter_box) = mapper.get_mut("/") {
            if counter_box.count > 0 {
                counter_box.count -= 1;
            }

            return Status::SUCCESS;
        }
    }

    unsafe {
        drop(Box::from_raw(from_this(this)));
    }

    Status::SUCCESS
}

pub extern "efiapi" fn delete(this: *mut FileProtocolV1) -> Status {
    if this.is_null() {
        return Status::INVALID_PARAMETER;
    }
    let this = unsafe { &mut *from_this(this) };
    let path = this.path.clone();

    let result = match this.kind {
        HandleKind::Dir => axfs::api::remove_dir(&path),
        HandleKind::File => axfs::api::remove_file(&path),
    };

    if let Err(_e) = result {
        return Status::WARN_DELETE_FAILURE;
    }

    unsafe {
        drop(Box::from_raw(from_this(this.protocol)));
    }

    Status::SUCCESS
}

pub extern "efiapi" fn read(
    this: *mut FileProtocolV1,
    buffer_size: *mut usize,
    buffer: *mut c_void,
) -> Status {
    if this.is_null() || buffer_size.is_null() {
        return Status::INVALID_PARAMETER;
    }

    let this = unsafe { &mut *from_this(this) };

    if !this.mode.contains(FileMode::READ) {
        return Status::ACCESS_DENIED;
    }

    match this.kind {
        HandleKind::Dir => {
            // TODO: implement directory reading
            // The spec does not seem to explicitly mention directory entries,
            // and axfs has limited capabilities and may not be able to construct valid information,
            // so it is not implemented yet.
            return Status::UNSUPPORTED;
        }
        HandleKind::File => {
            if buffer.is_null() {
                return Status::INVALID_PARAMETER;
            }

            let want = unsafe { *buffer_size };
            if want == 0 {
                return Status::SUCCESS;
            }

            let data = match axfs::api::read(&this.path) {
                Ok(v) => v,
                Err(_) => return Status::DEVICE_ERROR,
            };

            let len = data.len();
            let pos = this.position;

            if pos > len {
                return Status::DEVICE_ERROR;
            }

            let remain = len - pos;
            let take = core::cmp::min(remain, want);

            if take > 0 {
                let src = &data[pos..pos + take];
                let dst = unsafe { core::slice::from_raw_parts_mut(buffer as *mut u8, take) };
                dst.copy_from_slice(src);
                this.position = pos + take;
            }

            unsafe { *buffer_size = take };
            Status::SUCCESS
        }
    }
}

pub extern "efiapi" fn write(
    this: *mut FileProtocolV1,
    buffer_size: *mut usize,
    buffer: *const c_void,
) -> Status {
    if this.is_null() || buffer_size.is_null() {
        return Status::INVALID_PARAMETER;
    }

    let this = unsafe { &mut *from_this(this) };
    if !this.mode.contains(FileMode::WRITE) {
        return Status::ACCESS_DENIED;
    }

    match this.kind {
        HandleKind::Dir => {
            return Status::UNSUPPORTED;
        }
        HandleKind::File => {
            let want = unsafe { *buffer_size };
            if want == 0 {
                return Status::SUCCESS;
            }

            if buffer.is_null() {
                return Status::INVALID_PARAMETER;
            }

            let mut data = match axfs::api::read(&this.path) {
                Ok(v) => v,
                Err(_) => Vec::new(),
            };

            let pos = this.position;
            if pos > data.len() {
                return Status::DEVICE_ERROR;
            }

            if data.len() < pos {
                data.resize(pos, 0);
            }

            let end = pos.saturating_add(want);
            if data.len() < end {
                data.resize(end, 0);
            }

            let src = unsafe { core::slice::from_raw_parts(buffer as *const u8, want) };
            data[pos..end].copy_from_slice(src);
            if let Err(_e) = axfs::api::write(&this.path, &data) {
                return Status::DEVICE_ERROR;
            }
            this.position = end;
            unsafe { *buffer_size = want };
            Status::SUCCESS
        }
    }
}

pub extern "efiapi" fn get_position(this: *const FileProtocolV1, position: *mut u64) -> Status {
    if this.is_null() || position.is_null() {
        return Status::INVALID_PARAMETER;
    }
    let this = unsafe { &*from_this(this as *mut FileProtocolV1) };

    match this.kind {
        HandleKind::Dir => Status::UNSUPPORTED,
        HandleKind::File => {
            unsafe {
                *position = this.position as u64;
            }
            Status::SUCCESS
        }
    }
}

pub extern "efiapi" fn set_position(this: *mut FileProtocolV1, position: u64) -> Status {
    if this.is_null() {
        return Status::INVALID_PARAMETER;
    }
    let this = unsafe { &mut *from_this(this) };

    match this.kind {
        HandleKind::Dir => Status::UNSUPPORTED,
        HandleKind::File => {
            if position == u64::MAX {
                // 定位到 EOF：取当前文件长度
                let size = match axfs::api::metadata(&this.path) {
                    Ok(md) => md.len() as u64,
                    Err(_) => return Status::DEVICE_ERROR,
                };
                if let Ok(sz) = usize::try_from(size) {
                    this.position = sz;
                    Status::SUCCESS
                } else {
                    Status::INVALID_PARAMETER
                }
            } else {
                if let Ok(p) = usize::try_from(position) {
                    this.position = p;
                    Status::SUCCESS
                } else {
                    Status::INVALID_PARAMETER
                }
            }
        }
    }
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
