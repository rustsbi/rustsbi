#![forbid(unsafe_code)]

use runtime::hart::HartId;

#[cfg(all(feature = "payload", feature = "jump"))]
compile_error!("feature \"payload\" and feature \"jump\" cannot be enabled at the same time");

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    let hart_id = HartId::current()
        .map(|hart| hart.as_usize())
        .unwrap_or(usize::MAX);
    let trap = runtime::trap::DiagnosticSnapshot::capture();
    error!("Hart {} {info}", hart_id);
    error!("-----------------------------");
    error!("mcause:  {:?}", trap.cause);
    error!("mepc:    {:#018x}", trap.program_counter);
    error!("mtval:   {:#018x}", trap.trap_value);
    error!("-----------------------------");
    error!("System shutdown scheduled due to RustSBI panic");
    loop {}
}

#[cold]
pub fn stop() -> ! {
    loop {
        core::hint::spin_loop()
    }
}

#[cold]
pub(crate) fn hart_initialization(error: impl core::fmt::Display) -> ! {
    let hart_id = HartId::current().expect("BUG: initializing hart is outside the boot topology");
    error!("Hart {} initialization failed: {error}", hart_id.as_usize());
    stop()
}

/// Reports next-stage policy failures after the platform console is ready.
#[cold]
pub(crate) fn next_stage(error: crate::next_stage::Error) -> ! {
    #[cfg(not(any(feature = "payload", feature = "jump")))]
    {
        use crate::next_stage::{DynamicPolicyViolation, Error};
        use runtime::boot::DynamicReadError;

        match error {
            Error::DynamicRead(DynamicReadError::InvalidAddress { address }) => {
                error!(
                    "No dynamic information available at address {:#x}",
                    address.as_usize()
                );
            }
            Error::DynamicRead(DynamicReadError::Access { address, source }) => {
                error!(
                    "No dynamic information available at address {:#x}",
                    address.as_usize()
                );
                error!("* handoff access failed: {source:?}");
            }
            Error::DynamicRead(DynamicReadError::InvalidHeader {
                invalid_magic,
                unsupported_version,
            }) => {
                error!("No valid dynamic information available:");
                if let Some(magic) = invalid_magic {
                    error!("* dynamic information has invalid magic number {magic:#x}");
                }
                if let Some(version) = unsupported_version {
                    error!("* dynamic information version {version} is unsupported");
                }
            }
            Error::InvalidDynamic { info, violation } => {
                error!("Invalid data in dynamic information:");
                if matches!(
                    violation,
                    DynamicPolicyViolation::PrivilegeMode
                        | DynamicPolicyViolation::AddressAndPrivilegeMode
                ) {
                    error!("* dynamic information contains invalid privilege mode");
                }
                if matches!(
                    violation,
                    DynamicPolicyViolation::Address
                        | DynamicPolicyViolation::AddressAndPrivilegeMode
                ) {
                    error!("* dynamic information contains invalid next jump address");
                }
                let mode_name = match info.next_mode {
                    3 => "Machine",
                    1 => "Supervisor",
                    0 => "User",
                    _ => "Invalid",
                };
                error!(
                    "@ help: dynamic information contains magic value {:#x}, version {}, next jump address {:#x}, next privilege mode {} ({}), options {:#x}, boot hart ID {}",
                    info.magic,
                    info.version,
                    info.next_addr,
                    info.next_mode,
                    mode_name,
                    info.options,
                    info.boot_hart
                );
            }
        }
        crate::sbi::reset::fail()
    }
    #[cfg(any(feature = "payload", feature = "jump"))]
    match error {}
}
