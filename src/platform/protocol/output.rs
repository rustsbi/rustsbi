#[repr(C)]
pub struct EfiSimpleTextOutputProtocol {
    pub reset: u64,
    pub output_string: EfiTextString,
    pub test_string: u64,
    pub query_mode: u64,
    pub set_mode: u64,
    pub set_attribute: u64,
    pub clear_screen: u64,
    pub set_cursor_position: u64,
    pub enable_cursor: u64,
    pub mode: u64,
}

pub type EfiTextString =
    extern "efiapi" fn(this: *mut EfiSimpleTextOutputProtocol, string: *const u16) -> u64;
