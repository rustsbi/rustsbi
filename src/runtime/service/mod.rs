use axsync::Mutex;
use lazyinit::LazyInit;
use uefi_raw::table::{boot::BootServices, runtime::RuntimeServices};

use crate::runtime::service::{
    boot_service::{
        allocate_pages, allocate_pool, calculate_crc32, check_event, close_event, close_protocol,
        connect_controller, copy_mem, create_event, create_event_ex, disconnect_controller, exit,
        exit_boot_services, free_pages, free_pool, get_memory_map, get_next_monotonic_count,
        handle_protocol, install_configuration_table, install_multiple_protocol_interfaces,
        install_protocol_interface, load_image, locate_device_path, locate_handle,
        locate_handle_buffer, locate_protocol, open_protocol, open_protocol_information,
        protocols_per_handle, raise_tpl, register_protocol_notify, reinstall_protocol_interface,
        restore_tpl, set_mem, set_timer, set_watchdog_timer, signal_event, stall, start_image,
        uninstall_multiple_protocol_interfaces, uninstall_protocol_interface, unload_image,
        wait_for_event,
    },
    runtime_service::{
        convert_pointer, get_next_high_monotonic_count, get_next_variable_name, get_time,
        get_variable, get_wakeup_time, query_capsule_capabilities, query_variable_info,
        reset_system, set_time, set_variable, set_virtual_address_map, set_wakeup_time,
        update_capsule,
    },
};

pub(crate) mod boot_service;
pub(crate) mod runtime_service;

static BOOT_SERVICE: LazyInit<Mutex<RuntimeServices>> = LazyInit::new();
static RUNTIME_SERVICE: LazyInit<Mutex<RuntimeServices>> = LazyInit::new();

fn init_service() {
    BOOT_SERVICE.init_once(Mutex::new(BootServices {
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
        reserved: None,
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
        create_event_ex,
    }));

    RUNTIME_SERVICE.init_once(Mutex::new(RuntimeServices {
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
    }));
}

pub fn get_boot_service() -> *mut BootServices {
    BOOT_SERVICE
        .get()
        .expect("BootService not initialized")
        .lock()
        .as_mut()
}

pub fn get_runtime_service() -> *mut RuntimeServices {
    RUNTIME_SERVICE
        .get()
        .expect("RuntimeService not initialized")
        .lock()
        .as_mut()
}
