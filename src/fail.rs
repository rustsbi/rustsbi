use crate::dynamic;
use riscv::register::mstatus;

#[cold]
pub fn invalid_dynamic_info(err: dynamic::DynamicError) -> (mstatus::MPP, usize) {
    error!("Invalid dynamic information:");
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
        "help: dynamic information contains magic value 0x{:x}, version {}, next jump address 0x{:x}, next privilege mode {} ({}), options {:x}",
        err.bad_info.magic, err.bad_info.version, err.bad_info.next_addr, err.bad_info.next_mode, explain_next_mode, err.bad_info.options
    );
    loop {
        core::hint::spin_loop()
    }
}

#[cold]
pub fn no_dynamic_info_available(err: dynamic::DynamicReadError) -> dynamic::DynamicInfo {
    error!(
        "no dynamic information available at address 0x{:x}",
        err.bad_paddr
    );
    loop {
        core::hint::spin_loop()
    }
}
