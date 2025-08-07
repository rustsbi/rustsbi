use core::ffi::c_void;

use axsync::Mutex;
use lazyinit::LazyInit;
use uefi_raw::{
    Boolean, Status,
    protocol::block::{BlockIoMedia, BlockIoProtocol, Lba},
};

use alloc::boxed::Box;

static BLOCK_IO: LazyInit<Mutex<BlockIo>> = LazyInit::new();

#[derive(Debug)]
pub struct BlockIo {
    protocol: &'static mut BlockIoProtocol,
    protocol_raw: *mut BlockIoProtocol,
}

impl BlockIo {
    pub fn new() -> Self {
        let media: *const BlockIoMedia = core::ptr::null(); // stub; replace with actual media if needed

        let protocol = BlockIoProtocol {
            revision: 0x00010000,
            media,
            reset,
            read_blocks,
            write_blocks,
            flush_blocks,
        };

        let protocol_raw = Box::into_raw(Box::new(protocol));
        let protocol = unsafe { &mut *protocol_raw };

        Self {
            protocol,
            protocol_raw,
        }
    }

    pub fn get_protocol(&self) -> *mut BlockIoProtocol {
        self.protocol_raw
    }
}

unsafe impl Send for BlockIo {}
unsafe impl Sync for BlockIo {}

pub extern "efiapi" fn reset(
    _this: *mut BlockIoProtocol,
    _extended_verification: Boolean,
) -> Status {
    Status::UNSUPPORTED
}

pub extern "efiapi" fn read_blocks(
    _this: *const BlockIoProtocol,
    _media_id: u32,
    _lba: Lba,
    _buffer_size: usize,
    _buffer: *mut c_void,
) -> Status {
    Status::UNSUPPORTED
}

pub extern "efiapi" fn write_blocks(
    _this: *mut BlockIoProtocol,
    _media_id: u32,
    _lba: Lba,
    _buffer_size: usize,
    _buffer: *const c_void,
) -> Status {
    Status::UNSUPPORTED
}

pub extern "efiapi" fn flush_blocks(_this: *mut BlockIoProtocol) -> Status {
    Status::UNSUPPORTED
}
