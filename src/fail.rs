use crate::{
    dt::{self, ParseDeviceTreeError, Tree},
    dynamic, reset,
};
use riscv::register::mstatus;
use serde_device_tree::Dtb;

#[cold]
pub fn device_tree_format(err: dt::ParseDeviceTreeError) -> Dtb {
    match err {
        ParseDeviceTreeError::Format => error!("- FDT format error"),
    }
    reset::fail()
}

#[cold]
pub fn device_tree_deserialize<'a>(err: serde_device_tree::Error) -> Tree<'a> {
    error!("Device tree deserialization error: {:?}", err);
    reset::fail()
}

#[cold]
pub fn invalid_dynamic_data(err: dynamic::DynamicError) -> (mstatus::MPP, usize) {
    error!("Invalid data in dynamic information:");
    if err.invalid_mpp {
        error!("- dynamic information contains invalid privilege mode");
    }
    if err.invalid_next_addr {
        error!("- dynamic information contains invalid next jump address");
    }
    let explain_next_mode = match err.bad_info.next_mode {
        3 => "Machine",
        1 => "Supervisor",
        0 => "User",
        _ => "Invalid",
    };
    error!(
        "help: dynamic information contains magic value 0x{:x}, version {}, next jump address 0x{:x}, next privilege mode {} ({}), options {:x}, boot hart ID {}",
        err.bad_info.magic, err.bad_info.version, err.bad_info.next_addr, err.bad_info.next_mode, explain_next_mode, err.bad_info.options, err.bad_info.boot_hart
    );
    reset::fail()
}

#[cold]
pub fn no_dynamic_info_available(err: dynamic::DynamicReadError) -> dynamic::DynamicInfo {
    if let Some(bad_paddr) = err.bad_paddr {
        error!(
            "No dynamic information available at address 0x{:x}",
            bad_paddr
        );
    } else {
        error!("No valid dynamic information available:");
        if let Some(bad_magic) = err.bad_magic {
            error!(
                "- tried to identify dynamic information, but found invalid magic number 0x{:x}",
                bad_magic
            );
        }
        if let Some(bad_version) = err.bad_version {
            error!("- tries to identify version of dynamic information, but the version number {} is not supported", bad_version);
        }
        if err.bad_magic.is_none() {
            error!("help: magic number is valid")
        }
        if err.bad_version.is_none() {
            error!("help: dynamic information version is valid")
        }
    }
    reset::fail()
}
