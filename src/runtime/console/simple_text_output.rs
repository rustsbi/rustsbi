use uefi_raw::{Char16, Status, protocol::console::SimpleTextOutputProtocol};

extern crate alloc;

#[derive(Debug)]
#[repr(transparent)]
pub struct Output {
    inner: SimpleTextOutputProtocol,
}

impl Output {
    const GUID: uefi_raw::Guid = SimpleTextOutputProtocol::GUID;
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
