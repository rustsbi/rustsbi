use crate::platform::{efi::EfiTableHeader, protocol::output::EfiSimpleTextOutputProtocol};

#[repr(C)]
pub struct EfiSystemTable {
    pub hdr: EfiTableHeader,
    pub firmware_vendor: *const u16,
    pub firmware_revision: u32,
    pub console_in_handle: *mut core::ffi::c_void,
    pub con_in: u64,
    pub console_out_handle: *mut core::ffi::c_void,
    pub con_out: *mut EfiSimpleTextOutputProtocol,
    pub standard_error_handle: *mut core::ffi::c_void,
    pub std_err: u64,
    pub runtime_services: u64,
    pub boot_services: u64,
    pub number_of_table_entries: u64,
    pub configuration_table: u64,
}
