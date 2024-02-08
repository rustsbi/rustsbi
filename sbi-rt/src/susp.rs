//! Chapter 13. System Suspend Extension (EID #0x53555350 "SUSP")

use crate::binary::sbi_call_3;
use sbi_spec::{
    binary::SbiRet,
    susp::{EID_SUSP, SUSPEND},
};

/// Suspend the system based on provided `sleep_type`.
///
/// # Parameters
///
/// The `sleep_type` parameter specifies the sleep type.
///
/// | Type                    | Name           | Description
/// |:------------------------|:---------------|:----------------------------------------------
/// | 0                       | SUSPEND_TO_RAM | This is a "suspend to RAM" sleep type, similar to ACPI's S2 or S3. Entry requires all but the calling hart be in the HSM `STOPPED` state and all hart registers and CSRs saved to RAM.
/// | 0x00000001 - 0x7fffffff |                | Reserved for future use
/// | 0x80000000 - 0xffffffff |                | Platform-specific system sleep types
///
/// The `resume_addr` parameter points to a runtime-specified physical address,
/// where the hart can resume execution in supervisor-mode after a system suspend.
///
/// The `opaque` parameter is an XLEN-bit value that will be set in the `a1`
/// register when the hart resumes execution at `resume_addr` after a system
/// suspend.
///
/// # Return value
///
/// The possible return error codes returned in `SbiRet.error` are shown in
/// the table below:
///
/// | Return code               | Description
/// |:--------------------------|:----------------------------------------------
/// | `SbiRet::success()`       | The suspend request is accepted, and the system is suspended. The system will resume execution at `resume_addr` after the sleep period.
/// | `SbiRet::invalid_param()` | `sleep_type` is reserved or is platform-specific and unimplemented.
/// | `SbiRet::not_supported()` | `sleep_type` is not reserved and is implemented, but the platform does not support it due to one or more missing dependencies.
/// | `SbiRet::invalid_address()` | `resume_addr` is not valid, possibly due to the following reasons: + * It is not a valid physical address. + * Executable access to the address is prohibited by a physical memory protection mechanism or H-extension G-stage for supervisor mode.
/// | `SbiRet::denied()`        | The suspend request failed due to unsatisfied entry criteria.
/// | `SbiRet::failed()`        | The suspend request failed for unspecified or unknown other reasons.
///
/// This function is defined in RISC-V SBI Specification chapter 13.1.
#[inline]
pub fn system_suspend<T>(sleep_type: T, resume_addr: usize, opaque: usize) -> SbiRet
where
    T: SleepType,
{
    sbi_call_3(
        EID_SUSP,
        SUSPEND,
        sleep_type.raw() as _,
        resume_addr,
        opaque,
    )
}

/// A valid sleep type for system suspend.
pub trait SleepType {
    /// Get a raw value to pass to SBI environment.
    fn raw(&self) -> u32;
}

#[cfg(feature = "integer-impls")]
impl SleepType for u32 {
    #[inline]
    fn raw(&self) -> u32 {
        *self
    }
}

#[cfg(feature = "integer-impls")]
impl SleepType for i32 {
    #[inline]
    fn raw(&self) -> u32 {
        u32::from_ne_bytes(i32::to_ne_bytes(*self))
    }
}

/// Suspend to RAM as sleep type.
#[derive(Clone, Copy, Debug)]
pub struct SuspendToRam;

impl SleepType for SuspendToRam {
    #[inline]
    fn raw(&self) -> u32 {
        0
    }
}
