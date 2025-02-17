//! Chapter 18. SBI Firmware Features Extension (EID #0x46574654 "FWFT")

use crate::binary::{sbi_call_1, sbi_call_3};
use sbi_spec::{
    binary::SbiRet,
    fwft::{EID_FWFT, SET, GET},
};

/// Set the value of a specific firmware feature
///
/// # Parameters
/// - `feature`: The identifier of the feature to set.
/// - `value`: The value to set for the feature.
/// - `flags`: Flags to modify the behavior of the set operation.
///
/// # Return value
///
/// `SbiRet.value` is set to zero, and the possible error codes returned in `SbiRet.error` are shown in the table below:
///
/// | Error code                  | Description
/// |:----------------------------|:---------------------------------
/// | `SBI_SUCCESS`               | feature was set successfully.
/// | `SBI_ERR_NOT_SUPPORTED`     | feature is not reserved and valid, but the platform does not support it due to one or more missing dependencies (Hardware or SBI implementation).
/// | `SBI_ERR_INVALID_PARAM`     | Provided value or flags parameter is invalid.
/// | `SBI_ERR_DENIED`            | feature set operation failed because either:
///                             - it was denied by the SBI implementation
///                             - feature is reserved or is platform-specific and unimplemented
/// | `SBI_ERR_DENIED_LOCKED`     | feature set operation failed because the feature is locked.
/// | `SBI_ERR_FAILED`            | The set operation failed for unspecified or unknown other reasons.
///
/// This function is defined in RISC-V SBI Specification chapter 18.1.
#[inline]
pub fn fwft_set(feature: u32, value: usize, flags: usize) -> SbiRet {
    sbi_call_3(EID_FWFT, SET, feature as _, value, flags)
}

/// Get the value of a specific firmware feature
///
/// # Parameters
/// - `feature`: The identifier of the feature to get.
///
/// # Return value
///
/// `SbiRet.value` is set to zero, and the possible error codes returned in `SbiRet.error` are shown in the table below:
///
/// | Error code                  | Description
/// |:----------------------------|:---------------------------------
/// | `SBI_SUCCESS`               | Feature status was retrieved successfully.
/// | `SBI_ERR_NOT_SUPPORTED`     | feature is not reserved and valid, but the platform does not support it due to one or more missing dependencies (Hardware or SBI implementation).
/// | `SBI_ERR_DENIED`            | feature is reserved or is platform-specific and unimplemented.
/// | `SBI_ERR_FAILED`            | The get operation failed for unspecified or unknown other reasons.
///
/// This function is defined in RISC-V SBI Specification chapter 18.2.
#[inline]
pub fn fwft_get(feature: u32) -> SbiRet {
    sbi_call_1(EID_FWFT, GET, feature as _)
}