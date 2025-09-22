use core::ffi::c_void;

use axhal::mem::PhysAddr;
use uefi_raw::{
    Boolean, Char16, Event, Guid, Handle, PhysicalAddress, Status,
    protocol::device_path::DevicePathProtocol,
    table::boot::{
        EventNotifyFn, EventType, InterfaceType, MemoryDescriptor, MemoryType,
        OpenProtocolInformationEntry, TimerDelay, Tpl,
    },
};

use alloc::boxed::Box;

use crate::runtime::service::memory::AllocateType;

#[derive(Debug)]
pub struct Boot {
    services: &'static mut uefi_raw::table::boot::BootServices,
    services_raw: *mut uefi_raw::table::boot::BootServices,
}

impl Boot {
    pub fn new() -> Self {
        let services = uefi_raw::table::boot::BootServices {
            header: Default::default(),
            raise_tpl,
            restore_tpl,
            allocate_pages,
            free_pages,
            get_memory_map,
            allocate_pool,
            free_pool,
            create_event,
            set_timer,
            wait_for_event,
            signal_event,
            close_event,
            check_event,
            install_protocol_interface,
            reinstall_protocol_interface,
            uninstall_protocol_interface,
            handle_protocol,
            reserved: core::ptr::null_mut::<c_void>(),
            register_protocol_notify,
            locate_handle,
            locate_device_path,
            install_configuration_table,
            load_image,
            start_image,
            exit,
            unload_image,
            exit_boot_services,
            get_next_monotonic_count,
            stall,
            set_watchdog_timer,
            connect_controller,
            disconnect_controller,
            open_protocol,
            close_protocol,
            open_protocol_information,
            protocols_per_handle,
            locate_handle_buffer,
            locate_protocol,
            install_multiple_protocol_interfaces,
            uninstall_multiple_protocol_interfaces,
            calculate_crc32,
            copy_mem,
            set_mem,
            create_event_ex, // UEFI 2.0+
        };
        let services_raw = Box::into_raw(Box::new(services));
        let services = unsafe { &mut *services_raw };
        Boot {
            services,
            services_raw,
        }
    }

    pub fn get_services(&self) -> *mut uefi_raw::table::boot::BootServices {
        self.services_raw
    }
}

unsafe impl Send for Boot {}
unsafe impl Sync for Boot {}

impl Drop for Boot {
    fn drop(&mut self) {
        unsafe {
            drop(Box::from_raw(self.services_raw));
        }
    }
}

// Task Priority services
pub unsafe extern "efiapi" fn raise_tpl(_new_tpl: Tpl) -> Tpl {
    Tpl::APPLICATION
}
pub unsafe extern "efiapi" fn restore_tpl(_old_tpl: Tpl) {}

// Memory allocation functions
pub unsafe extern "efiapi" fn allocate_pages(
    alloc_ty: u32,
    mem_ty: MemoryType,
    count: usize,
    addr: *mut PhysicalAddress,
) -> Status {
    let alloc_ty = match AllocateType::try_from(alloc_ty) {
        Ok(t) => t,
        Err(_) => return Status::INVALID_PARAMETER,
    };

    let ptr = crate::runtime::service::memory::alloc_pages(alloc_ty, mem_ty, count);
    if ptr.is_null() {
        return Status::OUT_OF_RESOURCES;
    }

    unsafe { *addr = ptr as u64 };
    Status::SUCCESS
}
pub unsafe extern "efiapi" fn free_pages(addr: PhysicalAddress, pages: usize) -> Status {
    let phys_addr = PhysAddr::from_usize(addr.try_into().unwrap());
    let _ = crate::runtime::service::memory::free_pages(phys_addr, pages);

    Status::SUCCESS
}
pub unsafe extern "efiapi" fn get_memory_map(
    _size: *mut usize,
    _map: *mut MemoryDescriptor,
    _key: *mut usize,
    _desc_size: *mut usize,
    _desc_version: *mut u32,
) -> Status {
    Status::UNSUPPORTED
}
pub unsafe extern "efiapi" fn allocate_pool(
    _pool_type: MemoryType,
    _size: usize,
    _buffer: *mut *mut u8,
) -> Status {
    Status::UNSUPPORTED
}
pub unsafe extern "efiapi" fn free_pool(_buffer: *mut u8) -> Status {
    Status::UNSUPPORTED
}

// Event & timer functions
pub unsafe extern "efiapi" fn create_event(
    _ty: EventType,
    _notify_tpl: Tpl,
    _notify_func: Option<EventNotifyFn>,
    _notify_ctx: *mut c_void,
    _out_event: *mut Event,
) -> Status {
    Status::UNSUPPORTED
}
pub unsafe extern "efiapi" fn set_timer(
    _event: Event,
    _ty: TimerDelay,
    _trigger_time: u64,
) -> Status {
    Status::UNSUPPORTED
}
pub unsafe extern "efiapi" fn wait_for_event(
    _number_of_events: usize,
    _events: *mut Event,
    _out_index: *mut usize,
) -> Status {
    Status::UNSUPPORTED
}
pub unsafe extern "efiapi" fn signal_event(_event: Event) -> Status {
    Status::UNSUPPORTED
}
pub unsafe extern "efiapi" fn close_event(_event: Event) -> Status {
    Status::UNSUPPORTED
}
pub unsafe extern "efiapi" fn check_event(_event: Event) -> Status {
    Status::UNSUPPORTED
}

// Protocol handlers
pub unsafe extern "efiapi" fn install_protocol_interface(
    _handle: *mut Handle,
    _guid: *const Guid,
    _interface_type: InterfaceType,
    _interface: *const c_void,
) -> Status {
    Status::UNSUPPORTED
}
pub unsafe extern "efiapi" fn reinstall_protocol_interface(
    _handle: Handle,
    _protocol: *const Guid,
    _old_interface: *const c_void,
    _new_interface: *const c_void,
) -> Status {
    Status::UNSUPPORTED
}
pub unsafe extern "efiapi" fn uninstall_protocol_interface(
    _handle: Handle,
    _protocol: *const Guid,
    _interface: *const c_void,
) -> Status {
    Status::UNSUPPORTED
}
pub unsafe extern "efiapi" fn handle_protocol(
    _handle: Handle,
    _proto: *const Guid,
    _out_proto: *mut *mut c_void,
) -> Status {
    Status::UNSUPPORTED
}
pub unsafe extern "efiapi" fn register_protocol_notify(
    _protocol: *const Guid,
    _event: Event,
    _registration: *mut *const c_void,
) -> Status {
    Status::UNSUPPORTED
}
pub unsafe extern "efiapi" fn locate_handle(
    _search_ty: i32,
    _proto: *const Guid,
    _key: *const c_void,
    _buf_sz: *mut usize,
    _buf: *mut Handle,
) -> Status {
    Status::UNSUPPORTED
}
pub unsafe extern "efiapi" fn locate_device_path(
    _proto: *const Guid,
    _device_path: *mut *const DevicePathProtocol,
    _out_handle: *mut Handle,
) -> Status {
    Status::UNSUPPORTED
}
pub unsafe extern "efiapi" fn install_configuration_table(
    _guid_entry: *const Guid,
    _table_ptr: *const c_void,
) -> Status {
    Status::UNSUPPORTED
}

// Image services
pub unsafe extern "efiapi" fn load_image(
    _boot_policy: Boolean,
    _parent_image_handle: Handle,
    _device_path: *const DevicePathProtocol,
    _source_buffer: *const u8,
    _source_size: usize,
    _image_handle: *mut Handle,
) -> Status {
    Status::UNSUPPORTED
}
pub unsafe extern "efiapi" fn start_image(
    _image_handle: Handle,
    _exit_data_size: *mut usize,
    _exit_data: *mut *mut Char16,
) -> Status {
    Status::UNSUPPORTED
}
pub unsafe extern "efiapi" fn exit(
    _image_handle: Handle,
    _exit_status: Status,
    _exit_data_size: usize,
    _exit_data: *mut Char16,
) -> ! {
    loop {}
}
pub unsafe extern "efiapi" fn unload_image(_image_handle: Handle) -> Status {
    Status::UNSUPPORTED
}
pub unsafe extern "efiapi" fn exit_boot_services(_image_handle: Handle, _map_key: usize) -> Status {
    Status::UNSUPPORTED
}

// Misc services
pub unsafe extern "efiapi" fn get_next_monotonic_count(_count: *mut u64) -> Status {
    Status::UNSUPPORTED
}
pub unsafe extern "efiapi" fn stall(_microseconds: usize) -> Status {
    Status::UNSUPPORTED
}
pub unsafe extern "efiapi" fn set_watchdog_timer(
    _timeout: usize,
    _watchdog_code: u64,
    _data_size: usize,
    _watchdog_data: *const u16,
) -> Status {
    Status::UNSUPPORTED
}

// Driver support
pub unsafe extern "efiapi" fn connect_controller(
    _controller: Handle,
    _driver_image: Handle,
    _remaining_device_path: *const DevicePathProtocol,
    _recursive: Boolean,
) -> Status {
    Status::UNSUPPORTED
}
pub unsafe extern "efiapi" fn disconnect_controller(
    _controller: Handle,
    _driver_image: Handle,
    _child: Handle,
) -> Status {
    Status::UNSUPPORTED
}

// Protocol open/close
pub unsafe extern "efiapi" fn open_protocol(
    _handle: Handle,
    _protocol: *const Guid,
    _interface: *mut *mut c_void,
    _agent_handle: Handle,
    _controller_handle: Handle,
    _attributes: u32,
) -> Status {
    Status::UNSUPPORTED
}
pub unsafe extern "efiapi" fn close_protocol(
    _handle: Handle,
    _protocol: *const Guid,
    _agent_handle: Handle,
    _controller_handle: Handle,
) -> Status {
    Status::UNSUPPORTED
}
pub unsafe extern "efiapi" fn open_protocol_information(
    _handle: Handle,
    _protocol: *const Guid,
    _entry_buffer: *mut *const OpenProtocolInformationEntry,
    _entry_count: *mut usize,
) -> Status {
    Status::UNSUPPORTED
}

// Library services
pub unsafe extern "efiapi" fn protocols_per_handle(
    _handle: Handle,
    _protocol_buffer: *mut *mut *const Guid,
    _protocol_buffer_count: *mut usize,
) -> Status {
    Status::UNSUPPORTED
}
pub unsafe extern "efiapi" fn locate_handle_buffer(
    _search_ty: i32,
    _proto: *const Guid,
    _key: *const c_void,
    _no_handles: *mut usize,
    _buf: *mut *mut Handle,
) -> Status {
    Status::UNSUPPORTED
}
pub unsafe extern "efiapi" fn locate_protocol(
    _proto: *const Guid,
    _registration: *mut c_void,
    _out_proto: *mut *mut c_void,
) -> Status {
    Status::UNSUPPORTED
}
pub unsafe extern "C" fn install_multiple_protocol_interfaces(_handle: *mut Handle, ...) -> Status {
    Status::UNSUPPORTED
}

pub unsafe extern "C" fn uninstall_multiple_protocol_interfaces(_handle: Handle, ...) -> Status {
    Status::UNSUPPORTED
}

// CRC / memory
pub unsafe extern "efiapi" fn calculate_crc32(
    _data: *const c_void,
    _data_size: usize,
    _crc32: *mut u32,
) -> Status {
    Status::UNSUPPORTED
}
pub unsafe extern "efiapi" fn copy_mem(_dest: *mut u8, _src: *const u8, _len: usize) {}
pub unsafe extern "efiapi" fn set_mem(_buffer: *mut u8, _len: usize, _value: u8) {}

// New event (UEFI 2.0+)
pub unsafe extern "efiapi" fn create_event_ex(
    _ty: EventType,
    _notify_tpl: Tpl,
    _notify_fn: Option<EventNotifyFn>,
    _notify_ctx: *mut c_void,
    _event_group: *mut Guid,
    _out_event: *mut Event,
) -> Status {
    Status::UNSUPPORTED
}
