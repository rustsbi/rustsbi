use core::ffi::c_void;
use uefi_raw::{
    Char16, Guid, PhysicalAddress, Status,
    capsule::CapsuleHeader,
    table::{
        boot::MemoryDescriptor,
        runtime::{ResetType, TimeCapabilities, VariableAttributes},
    },
    time::Time,
};

use alloc::boxed::Box;

#[derive(Debug)]
pub struct Runtime {
    services: &'static mut uefi_raw::table::runtime::RuntimeServices,
    services_raw: *mut uefi_raw::table::runtime::RuntimeServices,
}

impl Runtime {
    pub fn new() -> Self {
        let services = uefi_raw::table::runtime::RuntimeServices {
            header: Default::default(),
            get_time,
            set_time,
            get_wakeup_time,
            set_wakeup_time,
            set_virtual_address_map,
            convert_pointer,
            get_variable,
            get_next_variable_name,
            set_variable,
            reset_system,
            update_capsule,
            query_capsule_capabilities,
            query_variable_info,
            get_next_high_monotonic_count,
        };
        let services_raw = Box::into_raw(Box::new(services));
        let services = unsafe { &mut *services_raw };
        Self {
            services,
            services_raw,
        }
    }

    pub fn get_services(&self) -> *mut uefi_raw::table::runtime::RuntimeServices {
        self.services_raw
    }
}

unsafe impl Send for Runtime {}
unsafe impl Sync for Runtime {}

impl Drop for Runtime {
    fn drop(&mut self) {
        unsafe {
            drop(Box::from_raw(self.services_raw));
        }
    }
}

// Time services
pub unsafe extern "efiapi" fn get_time(
    _time: *mut Time,
    _capabilities: *mut TimeCapabilities,
) -> Status {
    Status::UNSUPPORTED
}

pub unsafe extern "efiapi" fn set_time(_time: *const Time) -> Status {
    Status::UNSUPPORTED
}

pub unsafe extern "efiapi" fn get_wakeup_time(
    _enabled: *mut u8,
    _pending: *mut u8,
    _time: *mut Time,
) -> Status {
    Status::UNSUPPORTED
}

pub unsafe extern "efiapi" fn set_wakeup_time(_enable: u8, _time: *const Time) -> Status {
    Status::UNSUPPORTED
}

// Virtual memory services
pub unsafe extern "efiapi" fn set_virtual_address_map(
    _map_size: usize,
    _desc_size: usize,
    _desc_version: u32,
    _virtual_map: *mut MemoryDescriptor,
) -> Status {
    Status::UNSUPPORTED
}

pub unsafe extern "efiapi" fn convert_pointer(
    _debug_disposition: usize,
    _address: *mut *const c_void,
) -> Status {
    Status::UNSUPPORTED
}

// Variable services
pub unsafe extern "efiapi" fn get_variable(
    _variable_name: *const Char16,
    _vendor_guid: *const Guid,
    _attributes: *mut VariableAttributes,
    _data_size: *mut usize,
    _data: *mut u8,
) -> Status {
    Status::UNSUPPORTED
}

pub unsafe extern "efiapi" fn get_next_variable_name(
    _variable_name_size: *mut usize,
    _variable_name: *mut u16,
    _vendor_guid: *mut Guid,
) -> Status {
    Status::UNSUPPORTED
}

pub unsafe extern "efiapi" fn set_variable(
    _variable_name: *const Char16,
    _vendor_guid: *const Guid,
    _attributes: VariableAttributes,
    _data_size: usize,
    _data: *const u8,
) -> Status {
    Status::UNSUPPORTED
}

// Misc services
pub unsafe extern "efiapi" fn get_next_high_monotonic_count(_high_count: *mut u32) -> Status {
    Status::UNSUPPORTED
}

pub unsafe extern "efiapi" fn reset_system(
    _rt: ResetType,
    _status: Status,
    _data_size: usize,
    _data: *const u8,
) -> ! {
    loop {}
}

// Capsule services
pub unsafe extern "efiapi" fn update_capsule(
    _capsule_header_array: *const *const CapsuleHeader,
    _capsule_count: usize,
    _scatter_gather_list: PhysicalAddress,
) -> Status {
    Status::UNSUPPORTED
}

pub unsafe extern "efiapi" fn query_capsule_capabilities(
    _capsule_header_array: *const *const CapsuleHeader,
    _capsule_count: usize,
    _maximum_capsule_size: *mut u64,
    _reset_type: *mut ResetType,
) -> Status {
    Status::UNSUPPORTED
}

// Variable info
pub unsafe extern "efiapi" fn query_variable_info(
    _attributes: VariableAttributes,
    _maximum_variable_storage_size: *mut u64,
    _remaining_variable_storage_size: *mut u64,
    _maximum_variable_size: *mut u64,
) -> Status {
    Status::UNSUPPORTED
}
