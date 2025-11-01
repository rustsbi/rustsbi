use crate::riscv::current_hartid;
use serde_device_tree::Dtb;

use crate::devicetree;

use riscv::interrupt::machine::{Exception, Interrupt};
use riscv::register::{mcause::Trap, mepc, mtval};

#[cfg(all(feature = "payload", feature = "jump"))]
compile_error!("feature \"payload\" and feature \"jump\" cannot be enabled at the same time");

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    use ::riscv::register::*;
    error!("Hart {} {info}", current_hartid());
    error!("-----------------------------");
    error!("mcause:  {:?}", mcause::read().cause());
    error!("mepc:    {:#018x}", mepc::read());
    error!("mtval:   {:#018x}", mtval::read());
    error!("-----------------------------");
    error!("System shutdown scheduled due to RustSBI panic");
    loop {}
}

pub fn unsupported_trap(trap: Option<Trap<Interrupt, Exception>>) -> ! {
    error!("-----------------------------");
    error!("trap:    {trap:?}");
    error!("mepc:    {:#018x}", mepc::read());
    error!("mtval:   {:#018x}", mtval::read());
    error!("-----------------------------");
    panic!("Stopped with unsupported trap")
}

/// Handles device tree format parsing errors by logging and resetting.
#[cold]
pub fn device_tree_format(_err: devicetree::ParseDeviceTreeError) -> Dtb {
    loop {
        core::hint::spin_loop()
    }
}

#[cold]
pub fn device_tree_deserialize_root<'a>(
    _err: serde_device_tree::error::Error,
) -> serde_device_tree::buildin::Node<'a> {
    loop {
        core::hint::spin_loop()
    }
}

#[cold]
pub fn stop() -> ! {
    loop {
        core::hint::spin_loop()
    }
}

cfg_if::cfg_if! {
    if #[cfg(not(any(feature = "payload", feature = "jump")))] {
        use crate::firmware::dynamic;
        use crate::sbi::reset;
        use riscv::register::mstatus;
        /// Handles invalid dynamic information data by logging details and resetting.
        #[cold]
        pub fn invalid_dynamic_data(err: dynamic::DynamicError) -> (mstatus::MPP, usize) {
            error!("Invalid data in dynamic information:");
            if err.invalid_mpp {
                error!("* dynamic information contains invalid privilege mode");
            }
            if err.invalid_next_addr {
                error!("* dynamic information contains invalid next jump address");
            }
            let explain_next_mode = match err.bad_info.next_mode {
                3 => "Machine",
                1 => "Supervisor",
                0 => "User",
                _ => "Invalid",
            };
            error!(
                "@ help: dynamic information contains magic value 0x{:x}, version {}, next jump address 0x{:x}, next privilege mode {} ({}), options {:x}, boot hart ID {}",
                err.bad_info.magic, err.bad_info.version, err.bad_info.next_addr, err.bad_info.next_mode, explain_next_mode, err.bad_info.options, err.bad_info.boot_hart
            );
            reset::fail()
        }

        /// Handles case where dynamic information is not available by logging details and resetting.
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
                        "* tried to identify dynamic information, but found invalid magic number 0x{:x}",
                        bad_magic
                    );
                }
                if let Some(bad_version) = err.bad_version {
                    error!("* tries to identify version of dynamic information, but the version number {} is not supported", bad_version);
                }
                if err.bad_magic.is_none() {
                    error!("@ help: magic number is valid")
                }
                if err.bad_version.is_none() {
                    error!("@ help: dynamic information version is valid")
                }
            }
            reset::fail()
        }

        /// Fallback function that returns default dynamic info with boot_hart set to MAX.
        ///
        /// Used when dynamic info read fails but execution should continue.
        #[cold]
        #[allow(unused)]
        pub fn use_lottery(_err: dynamic::DynamicReadError) -> dynamic::DynamicInfo {
            dynamic::DynamicInfo {
                magic: 0,
                version: 0,
                next_addr: 0,
                next_mode: 0,
                options: 0,
                boot_hart: usize::MAX,
            }
        }
    }
}
