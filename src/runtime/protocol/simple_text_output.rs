use axsync::Mutex;
use lazyinit::LazyInit;
use uefi_raw::{
    Boolean, Char16, Status,
    protocol::console::{SimpleTextOutputMode, SimpleTextOutputProtocol},
};

extern crate alloc;
use alloc::boxed::Box;

static TEXT_OUTPUT: LazyInit<Mutex<Output>> = LazyInit::new();

#[derive(Debug)]
pub struct Output {
    protocol: &'static mut SimpleTextOutputProtocol,
    protocol_raw: *mut SimpleTextOutputProtocol,
}

impl Output {
    pub fn new() -> Self {
        let mode = Box::new(SimpleTextOutputMode::default());
        let mode_raw = Box::into_raw(mode);
        let protocol = SimpleTextOutputProtocol {
            reset,
            output_string,
            test_string,
            query_mode,
            set_mode,
            set_attribute,
            clear_screen,
            set_cursor_position,
            enable_cursor,
            mode: mode_raw,
        };
        let protocol_raw = Box::into_raw(Box::new(protocol));
        let protocol = unsafe { &mut *protocol_raw };
        Output {
            protocol,
            protocol_raw,
        }
    }

    pub fn get_protocol(&self) -> *mut SimpleTextOutputProtocol {
        let guard = TEXT_OUTPUT.lock();
        guard.protocol_raw
    }
}

unsafe impl Send for Output {}
unsafe impl Sync for Output {}

impl Drop for Output {
    fn drop(&mut self) {
        unsafe {
            let mode_raw = self.protocol.mode;
            drop(Box::from_raw(mode_raw));
            drop(Box::from_raw(self.protocol_raw));
        }
    }
}

pub fn init_simple_text_output() {
    TEXT_OUTPUT.init_once(Mutex::new(Output::new()));
}

pub fn get_simple_text_output() -> &'static Mutex<Output> {
    TEXT_OUTPUT.get().expect("SimpleTextOutput not initialized")
}

pub extern "efiapi" fn reset(_this: *mut SimpleTextOutputProtocol, _extended: Boolean) -> Status {
    // Resetting the console is a no-op in this implementation.
    Status::SUCCESS
}

pub extern "efiapi" fn output_string(
    _this: *mut SimpleTextOutputProtocol,
    string: *const u16,
) -> Status {
    unsafe {
        let mut len = 0;
        while *string.add(len) != 0 {
            len += 1;
        }
        let message = core::slice::from_raw_parts(string, len as usize).iter();
        let utf16_message = core::char::decode_utf16(message.cloned());
        let decoded_message: alloc::string::String =
            utf16_message.map(|r| r.unwrap_or('\u{FFFD}')).collect();
        info!("EFI Output: {}", decoded_message);
    }
    Status::SUCCESS
}

pub extern "efiapi" fn test_string(
    _this: *mut SimpleTextOutputProtocol,
    string: *const Char16,
) -> Status {
    if string.is_null() {
        return Status::INVALID_PARAMETER;
    }
    for i in 0..1024 {
        let c = unsafe { *string.add(i) };
        if c == 0 {
            return Status::SUCCESS;
        }

        // This part should be handled by the firmware,
        // we are limited to ascii characters based on current output device support.
        //
        // TODO: When more output devices and output formats are supported,
        // support for other encoding areas will be provided
        if c > 0x7F || (0xD800..=0xDFFF).contains(&c) {
            return Status::UNSUPPORTED;
        }
    }
    Status::UNSUPPORTED
}

pub extern "efiapi" fn query_mode(
    _this: *mut SimpleTextOutputProtocol,
    _mode: usize,
    _columns: *mut usize,
    _rows: *mut usize,
) -> Status {
    Status::UNSUPPORTED
}

pub extern "efiapi" fn set_mode(_this: *mut SimpleTextOutputProtocol, _mode: usize) -> Status {
    Status::UNSUPPORTED
}

pub extern "efiapi" fn set_attribute(
    _this: *mut SimpleTextOutputProtocol,
    _attribute: usize,
) -> Status {
    Status::UNSUPPORTED
}

pub extern "efiapi" fn clear_screen(_this: *mut SimpleTextOutputProtocol) -> Status {
    Status::UNSUPPORTED
}

pub extern "efiapi" fn set_cursor_position(
    _this: *mut SimpleTextOutputProtocol,
    _column: usize,
    _row: usize,
) -> Status {
    Status::UNSUPPORTED
}

pub extern "efiapi" fn enable_cursor(
    _this: *mut SimpleTextOutputProtocol,
    _visible: Boolean,
) -> Status {
    Status::UNSUPPORTED
}
