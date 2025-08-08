use alloc::boxed::Box;
use axsync::Mutex;
use lazyinit::LazyInit;

use uefi_raw::{
    Status,
    protocol::console::{
        GraphicsOutputBltOperation, GraphicsOutputBltPixel, GraphicsOutputModeInformation,
        GraphicsOutputProtocol, GraphicsOutputProtocolMode,
    },
};

static GOP: LazyInit<Mutex<GraphicsOutput>> = LazyInit::new();

#[derive(Debug)]
pub struct GraphicsOutput {
    protocol: &'static mut GraphicsOutputProtocol,
    protocol_raw: *mut GraphicsOutputProtocol,
}

impl GraphicsOutput {
    pub fn new() -> Self {
        let mode: *mut GraphicsOutputProtocolMode = core::ptr::null_mut();

        let protocol = GraphicsOutputProtocol {
            query_mode,
            set_mode,
            blt,
            mode,
        };

        let protocol_raw = Box::into_raw(Box::new(protocol));
        let protocol = unsafe { &mut *protocol_raw };

        Self {
            protocol,
            protocol_raw,
        }
    }

    pub fn get_protocol(&self) -> *mut GraphicsOutputProtocol {
        self.protocol_raw
    }
}

unsafe impl Send for GraphicsOutput {}
unsafe impl Sync for GraphicsOutput {}

pub unsafe extern "efiapi" fn query_mode(
    _this: *const GraphicsOutputProtocol,
    _mode_number: u32,
    _size_of_info: *mut usize,
    _info: *mut *const GraphicsOutputModeInformation,
) -> Status {
    Status::UNSUPPORTED
}

pub unsafe extern "efiapi" fn set_mode(
    _this: *mut GraphicsOutputProtocol,
    _mode_number: u32,
) -> Status {
    Status::UNSUPPORTED
}

pub unsafe extern "efiapi" fn blt(
    _this: *mut GraphicsOutputProtocol,
    _blt_buffer: *mut GraphicsOutputBltPixel,
    _blt_operation: GraphicsOutputBltOperation,
    _source_x: usize,
    _source_y: usize,
    _destination_x: usize,
    _destination_y: usize,
    _width: usize,
    _height: usize,
    _delta: usize,
) -> Status {
    Status::UNSUPPORTED
}
