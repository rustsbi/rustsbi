//! Chapter 10. System Reset Extension (EID #0x53525354 "SRST")

use crate::binary::sbi_call_2;

use sbi_spec::{
    binary::SbiRet,
    srst::{
        EID_SRST, RESET_REASON_NO_REASON, RESET_REASON_SYSTEM_FAILURE, RESET_TYPE_COLD_REBOOT,
        RESET_TYPE_SHUTDOWN, RESET_TYPE_WARM_REBOOT, SYSTEM_RESET,
    },
};

/// Reset the system based on provided `reset_type` and `reset_reason`.
///
/// This is a synchronous call and does not return if it succeeds.
///
/// # Warm reboot and cold reboot
///
/// When supervisor software is running natively, the SBI implementation is machine mode firmware.
/// In this case, shutdown is equivalent to physical power down of the entire system, and
/// cold reboot is equivalent to a physical power cycle of the entire system.
/// Further, warm reboot is equivalent to a power cycle of the main processor and parts of the system
/// but not the entire system.
///
/// For example, on a server class system with a BMC (board management controller),
/// a warm reboot will not power cycle the BMC whereas a cold reboot will definitely power cycle the BMC.
///
/// When supervisor software is running inside a virtual machine, the SBI implementation is a hypervisor.
/// The shutdown, cold reboot and warm reboot will behave functionally the same as the native case but might
/// not result in any physical power changes.
///
/// This function is defined in RISC-V SBI Specification chapter 10.1.
#[inline]
pub fn system_reset<T, R>(reset_type: T, reset_reason: R) -> SbiRet
where
    T: ResetType,
    R: ResetReason,
{
    sbi_call_2(
        EID_SRST,
        SYSTEM_RESET,
        reset_type.raw() as _,
        reset_reason.raw() as _,
    )
}

/// A valid type for system reset.
pub trait ResetType {
    /// Get a raw value to pass to SBI environment.
    fn raw(&self) -> u32;
}

#[cfg(feature = "integer-impls")]
impl ResetType for u32 {
    #[inline]
    fn raw(&self) -> u32 {
        *self
    }
}

#[cfg(feature = "integer-impls")]
impl ResetType for i32 {
    #[inline]
    fn raw(&self) -> u32 {
        u32::from_ne_bytes(i32::to_ne_bytes(*self))
    }
}

/// A valid reason for system reset.
pub trait ResetReason {
    /// Get a raw value to pass to SBI environment.
    fn raw(&self) -> u32;
}

#[cfg(feature = "integer-impls")]
impl ResetReason for u32 {
    #[inline]
    fn raw(&self) -> u32 {
        *self
    }
}

#[cfg(feature = "integer-impls")]
impl ResetReason for i32 {
    #[inline]
    fn raw(&self) -> u32 {
        u32::from_ne_bytes(i32::to_ne_bytes(*self))
    }
}

macro_rules! define_reset_param {
    ($($struct:ident($value:expr): $trait:ident #[$doc:meta])*) => {
        $(
            #[derive(Clone, Copy, Debug)]
            #[$doc]
            pub struct $struct;
            impl $trait for $struct {
                #[inline]
                fn raw(&self) -> u32 {
                    $value
                }
            }
        )*
    };
}

define_reset_param! {
    Shutdown(RESET_TYPE_SHUTDOWN): ResetType /// Shutdown as a reset type.
    ColdReboot(RESET_TYPE_COLD_REBOOT): ResetType /// Cold reboot as a reset type.
    WarmReboot(RESET_TYPE_WARM_REBOOT): ResetType /// Warm reboot as a reset type.
    NoReason(RESET_REASON_NO_REASON): ResetReason /// No reason as a reset reason.
    SystemFailure(RESET_REASON_SYSTEM_FAILURE): ResetReason /// System failure as a reset reason.
}
