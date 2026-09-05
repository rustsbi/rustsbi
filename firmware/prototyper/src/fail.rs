#![forbid(unsafe_code)]

use crate::riscv::current_hartid;

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
        pub fn invalid_dynamic_data(err: dynamic::ValidationError) -> (mstatus::MPP, usize) {
            error!("Invalid data in dynamic information:");
            if err.invalid_next_mode {
                error!("* dynamic information contains invalid privilege mode");
            }
            if err.invalid_next_address {
                error!("* dynamic information contains invalid next jump address");
            }
            let next_mode_name = match err.dynamic_info.next_mode {
                3 => "Machine",
                1 => "Supervisor",
                0 => "User",
                _ => "Invalid",
            };
            error!(
                "@ help: dynamic information contains magic value 0x{:x}, version {}, next jump address 0x{:x}, next privilege mode {} ({}), options {:x}, boot hart ID {}",
                err.dynamic_info.magic, err.dynamic_info.version, err.dynamic_info.next_addr, err.dynamic_info.next_mode, next_mode_name, err.dynamic_info.options, err.dynamic_info.boot_hart
            );
            reset::fail()
        }

        /// Handles case where dynamic information is not available by logging details and resetting.
        #[cold]
        pub fn no_dynamic_info_available(err: dynamic::ReadError) -> dynamic::DynamicInfo {
            if let Some(invalid_address) = err.invalid_address {
                error!(
                    "No dynamic information available at address 0x{:x}",
                    invalid_address
                );
            } else {
                error!("No valid dynamic information available:");
                if let Some(invalid_magic) = err.invalid_magic {
                    error!(
                        "* tried to identify dynamic information, but found invalid magic number 0x{:x}",
                        invalid_magic
                    );
                }
                if let Some(unsupported_version) = err.unsupported_version {
                    error!("* tries to identify version of dynamic information, but the version number {} is not supported", unsupported_version);
                }
                if err.invalid_magic.is_none() {
                    error!("@ help: magic number is valid")
                }
                if err.unsupported_version.is_none() {
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
        pub fn fallback_to_boot_hart_election(
            _error: dynamic::ReadError,
        ) -> dynamic::DynamicInfo {
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
